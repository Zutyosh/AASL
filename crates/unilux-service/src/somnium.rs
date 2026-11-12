//! Somnium Space's own web API (the one its launcher talks to): players
//! online, which needs no account, and upcoming events, which need the
//! player's session. The session token is kept in a file only the user can
//! read and is never logged.
//!
//! Somnium Space staff confirmed (November 2026) that Unilux may talk to this
//! API; it is due to be documented. Out of scope by their terms: signing the
//! official launcher in on the user's behalf (its registry session is never
//! touched).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use unilux_core::{Error, Result};

use crate::Service;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnlineStats {
    #[serde(rename = "CurrentOnline")]
    pub current: u32,
    #[serde(rename = "Peak24Hours")]
    pub peak_24h: u32,
    #[serde(rename = "AllTimePeak")]
    pub all_time_peak: u32,
    #[serde(rename = "AllTimePeakDescription", default)]
    pub all_time_peak_when: String,
}

/// One point of the players-online history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnlinePoint {
    #[serde(rename = "Label")]
    pub label: String,
    #[serde(rename = "Value")]
    pub value: u32,
}

/// The account a saved session belongs to, read from the token itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Account {
    pub name: String,
    pub user_id: Option<String>,
    /// Unix seconds; None when the token does not say.
    pub expires: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpcomingEvent {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub creator: Option<String>,
    pub attendees: Option<u32>,
    /// Unix seconds.
    pub start: u64,
    pub end: Option<u64>,
    pub image_url: Option<String>,
}

/// Where a sign-in stands.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum LoginStep {
    Done(Account),
    /// A second factor is needed; answer with `somnium_verify`.
    Code(TwoFactor),
}

/// The second factors the account has.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TwoFactor {
    pub sms: bool,
    pub authenticator: bool,
    /// Masked number the SMS goes to, as Somnium shows it.
    pub phone: Option<String>,
}

/// Somnium's codes for each kind of second factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TwoFactorMethod {
    BackupCode = 0,
    Sms = 1,
    Authenticator = 2,
}

#[derive(Serialize, Deserialize)]
struct Session {
    token: String,
}

impl Service {
    fn somnium_url(&self, path: &str) -> String {
        format!(
            "{}/{path}",
            self.inner.sources.somnium_api.trim_end_matches('/')
        )
    }

    fn http(&self) -> reqwest::Client {
        unilux_components::Downloader::new().client().clone()
    }

    async fn somnium_get(&self, path: &str, token: Option<&str>) -> Result<Value> {
        let mut req = self
            .http()
            .get(self.somnium_url(path))
            .timeout(std::time::Duration::from_secs(20));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.map_err(net)?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(net)?;
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::Invalid(
                "Somnium session expired: sign in again.".into(),
            ));
        }
        if !status.is_success() {
            return Err(Error::Network(format!("Somnium API {path}: HTTP {status}")));
        }
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::Network(format!("Somnium API {path}: {e}")))
    }

    /// Players online now, the 24 h peak and the all-time peak. Public.
    pub async fn online_stats(&self) -> Result<OnlineStats> {
        let v = self
            .somnium_get("OnlineStatistic/getOverview", None)
            .await?;
        serde_json::from_value(v).map_err(|e| Error::Network(format!("online statistics: {e}")))
    }

    /// Players online over time: zoom 0 is two days by hour, 1 a week and
    /// 2 a month by day. Public.
    pub async fn online_history(&self, zoom: u8) -> Result<Vec<OnlinePoint>> {
        let v = self
            .somnium_get(&format!("OnlineStatistic/getChart?zoom={zoom}"), None)
            .await?;
        serde_json::from_value(v).map_err(|e| Error::Network(format!("online history: {e}")))
    }

    fn session_file(&self) -> PathBuf {
        self.inner.paths.config_dir.join("somnium-session.json")
    }

    fn session_token(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.session_file()).ok()?;
        serde_json::from_str::<Session>(&text)
            .ok()
            .map(|s| s.token)
            .filter(|t| !t.is_empty())
    }

    /// The signed-in Somnium account, if a session is saved.
    pub fn somnium_account(&self) -> Option<Account> {
        account_from_token(&self.session_token()?)
    }

    async fn somnium_post(&self, path: &str, body: Value) -> Result<Value> {
        let resp = self
            .http()
            .post(self.somnium_url(path))
            .timeout(std::time::Duration::from_secs(20))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(net)?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(net)?;
        let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !status.is_success() {
            let why =
                text(&v, &["Description", "Title"]).unwrap_or_else(|| format!("HTTP {status}"));
            return Err(Error::Invalid(format!("Somnium Space: {why}")));
        }
        Ok(v)
    }

    /// Sign in with a Somnium Space login (e-mail or user name) and password,
    /// as the official launcher does. Accounts with two-factor sign-in get
    /// a challenge to answer with `somnium_verify`. Only the session token
    /// is kept.
    pub async fn somnium_login(&self, login: &str, password: &str) -> Result<LoginStep> {
        let body = serde_json::json!({ "Login": login.trim(), "Password": password });
        let v = self.somnium_post("Account/loginLauncher", body).await?;
        let token = session_in(&v)
            .ok_or_else(|| Error::Invalid("Somnium Space answered without a session.".into()))?;
        let sms = field(&v, "IsSMSEnabled").and_then(Value::as_bool) == Some(true);
        let app = field(&v, "IsAuthenticatorConnected").and_then(Value::as_bool) == Some(true);
        // Same rule as somniumspace.com: a token without the account behind
        // it, on an account with a second factor, is only a temporary one.
        if field(&v, "Id").is_none() && (sms || app) {
            *self.inner.somnium_2fa.lock().unwrap() = Some(token);
            return Ok(LoginStep::Code(TwoFactor {
                sms,
                authenticator: app,
                phone: text(&v, &["PhoneNumberForTwoFactor"]),
            }));
        }
        self.finish_login(&token, login)
    }

    fn finish_login(&self, token: &str, login: &str) -> Result<LoginStep> {
        let account = account_from_token(token).unwrap_or(Account {
            name: login.trim().to_string(),
            user_id: None,
            expires: None,
        });
        self.save_session(token)?;
        *self.inner.somnium_2fa.lock().unwrap() = None;
        Ok(LoginStep::Done(account))
    }

    fn pending_2fa(&self) -> Result<String> {
        self.inner
            .somnium_2fa
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| {
                Error::Invalid("Sign in again: the verification step has expired.".into())
            })
    }

    /// Ask Somnium Space to text the verification code.
    pub async fn somnium_send_sms(&self) -> Result<()> {
        let temp = self.pending_2fa()?;
        let body = serde_json::json!({ "TemporaryToken": temp, "Code": "", "Type": TwoFactorMethod::Sms as u8 });
        self.somnium_post("Account/sendMessage2fa", body)
            .await
            .map(|_| ())
    }

    /// Answer the two-factor challenge of the last `somnium_login`.
    pub async fn somnium_verify(&self, code: &str, method: TwoFactorMethod) -> Result<Account> {
        let temp = self.pending_2fa()?;
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        if code.is_empty() {
            return Err(Error::Invalid("Enter the verification code.".into()));
        }
        let body = serde_json::json!({
            "TemporaryToken": temp,
            "Code": code,
            "Type": method as u8,
            "TrustDevice": false,
        });
        let v = self.somnium_post("Account/verify2fa", body).await?;
        let token = session_in(&v)
            .ok_or_else(|| Error::Invalid("Somnium Space answered without a session.".into()))?;
        match self.finish_login(&token, "")? {
            LoginStep::Done(a) => Ok(a),
            LoginStep::Code(_) => unreachable!(),
        }
    }

    fn save_session(&self, token: &str) -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let path = self.session_file();
        let dir = path.parent().unwrap_or(std::path::Path::new("."));
        std::fs::create_dir_all(dir).map_err(|e| Error::io("creating the config folder", e))?;
        let tmp = path.with_extension("tmp");
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| Error::io("saving the Somnium session", e))?;
        let text = serde_json::to_vec(&Session {
            token: token.to_string(),
        })
        .map_err(|e| Error::Invalid(e.to_string()))?;
        f.write_all(&text)
            .map_err(|e| Error::io("saving the Somnium session", e))?;
        std::fs::rename(&tmp, &path).map_err(|e| Error::io("saving the Somnium session", e))
    }

    /// Forget the saved session.
    pub fn somnium_logout(&self) -> Result<()> {
        match std::fs::remove_file(self.session_file()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(Error::io("removing the Somnium session", e))
            }
            _ => Ok(()),
        }
    }

    /// Events still to come, soonest first, as the official launcher's
    /// home page lists them. Needs a signed-in account.
    pub async fn upcoming_events(&self) -> Result<Vec<UpcomingEvent>> {
        let token = self
            .session_token()
            .ok_or_else(|| Error::Invalid("Sign in to Somnium Space to see events.".into()))?;
        let v = self.somnium_get("Event/mainPage", Some(&token)).await?;
        let now = unix_now();
        let mut out = Vec::new();
        collect_events(&v, &mut out);
        out.retain(|e| e.end.unwrap_or(e.start + 3600) > now);
        out.sort_by_key(|e| e.start);
        out.dedup_by(|a, b| a.id == b.id && a.start == b.start);
        Ok(out)
    }

    /// An event banner, downloaded once into the cache.
    pub async fn event_image(&self, ev: &UpcomingEvent) -> Result<PathBuf> {
        let url = ev
            .image_url
            .as_deref()
            .ok_or_else(|| Error::Invalid("this event has no picture".into()))?;
        let dir = self.inner.paths.cache_dir.join("events");
        let safe: String = format!("{}-{}", ev.id, ev.start)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let p = dir.join(format!("{safe}.img"));
        if p.is_file() {
            return Ok(p);
        }
        std::fs::create_dir_all(&dir).map_err(|e| Error::io("creating the event cache", e))?;
        let tmp = dir.join(format!("{safe}.download"));
        unilux_components::Downloader::new()
            .fetch(url, &tmp, None, |_, _| {})
            .await?;
        std::fs::rename(&tmp, &p).map_err(|e| Error::io("saving an event picture", e))?;
        Ok(p)
    }
}

fn net(e: reqwest::Error) -> Error {
    Error::Network(e.without_url().to_string())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// A key, matched without regard to case (the API is PascalCase today).
fn field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
        .filter(|v| !v.is_null())
}

fn text(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| match field(v, k)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        o @ Value::Object(_) => text(o, &["Name", "Title"]),
        _ => None,
    })
}

/// Walk the response and keep every object that looks like an event, so a
/// change in how the page groups them does not hide them.
fn collect_events(v: &Value, out: &mut Vec<UpcomingEvent>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| collect_events(x, out)),
        Value::Object(o) => {
            if let Some(ev) = event_from(v) {
                out.push(ev);
            }
            o.values().for_each(|x| collect_events(x, out));
        }
        _ => {}
    }
}

fn event_from(v: &Value) -> Option<UpcomingEvent> {
    let start = field(v, "DateTimeStart")?.as_str().and_then(parse_time)?;
    let name = text(v, &["Name", "Title"])?;
    let attendees = match field(v, "Attendees").or_else(|| field(v, "UserCount")) {
        Some(Value::Number(n)) => n.as_u64().map(|n| n as u32),
        Some(Value::Array(a)) => Some(a.len() as u32),
        _ => None,
    };
    let image_url = v.as_object()?.iter().find_map(|(k, x)| {
        let k = k.to_ascii_lowercase();
        let s = x.as_str()?;
        ((k.contains("image") || k.contains("banner"))
            && !k.contains("profile")
            && s.starts_with("https://"))
        .then(|| s.to_string())
    });
    Some(UpcomingEvent {
        id: text(v, &["Id"]).unwrap_or_else(|| name.clone()),
        name,
        description: text(v, &["Description"]),
        location: text(v, &["Location", "LocationName"]),
        creator: text(v, &["CreatorNickname"]),
        attendees,
        start,
        end: field(v, "DateTimeEnd")
            .and_then(Value::as_str)
            .and_then(parse_time),
        image_url,
    })
}

/// `2026-11-12T20:00:00`, with optional fraction and offset; no offset
/// means UTC.
fn parse_time(s: &str) -> Option<u64> {
    let s = s.trim();
    let (date, rest) = s.split_once(['T', ' '])?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, mo, da) = (d.next()??, d.next()??, d.next()??);
    let (clock, offset) = match rest.find(['Z', '+', '-']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let mut c = clock.split(':');
    let h: i64 = c.next()?.parse().ok()?;
    let mi: i64 = c.next()?.parse().ok()?;
    let se: i64 = c
        .next()
        .map(|x| x.split('.').next().unwrap_or("0"))
        .unwrap_or("0")
        .parse()
        .ok()?;
    let off = match offset.as_bytes().first() {
        Some(b'+') | Some(b'-') => {
            let sign = if offset.starts_with('-') { -1 } else { 1 };
            let (oh, om) = offset[1..].split_once(':').unwrap_or((&offset[1..], "0"));
            sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60)
        }
        _ => 0,
    };
    // Days from civil (Howard Hinnant's algorithm).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    u64::try_from(days * 86400 + h * 3600 + mi * 60 + se - off).ok()
}

/// The session token of a sign-in answer, without any "Bearer " prefix.
fn session_in(v: &Value) -> Option<String> {
    let t = match v {
        Value::String(s) => s.clone(),
        _ => text(v, &["Token", "AccessToken", "JwtToken"])?,
    };
    let t = t.trim().trim_start_matches("Bearer ").trim().to_string();
    (!t.is_empty()).then_some(t)
}

fn account_from_token(token: &str) -> Option<Account> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64url(payload)?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    Some(Account {
        name: text(&v, &["unique_name", "name", "nickname"])?,
        user_id: text(&v, &["nameid", "sub"]),
        expires: field(&v, "exp").and_then(Value::as_u64),
    })
}

fn base64url(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes().filter(|c| *c != b'=') {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn svc(home: &std::path::Path, uri: &str) -> Service {
        let h = home.display().to_string();
        let paths = unilux_core::Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
        Service::builder(paths)
            .host(Arc::new(unilux_system::FakeHost::new()))
            .sources(unilux_components::Sources {
                somnium_api: format!("{uri}/api"),
                ..Default::default()
            })
            .log_file(false)
            .build()
            .unwrap()
    }

    fn jwt(payload: &str) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut s = String::new();
        for ch in payload.as_bytes().chunks(3) {
            let n = ch
                .iter()
                .enumerate()
                .fold(0u32, |a, (i, b)| a | (*b as u32) << (16 - 8 * i));
            for i in 0..=ch.len() {
                s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        format!("eyJhbGciOiJIUzI1NiJ9.{s}.sig")
    }

    #[test]
    fn times_parse_with_and_without_offset() {
        assert_eq!(parse_time("1970-01-01T00:00:00"), Some(0));
        assert_eq!(parse_time("2026-11-12T20:00:00Z"), Some(1794513600));
        assert_eq!(
            parse_time("2026-11-12T21:00:00.123+01:00"),
            Some(1794513600)
        );
        assert_eq!(parse_time("nope"), None);
    }

    #[tokio::test]
    async fn online_stats_are_public() {
        let server = MockServer::start().await;
        Mock::given(path("/api/OnlineStatistic/getOverview"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"CurrentOnline":39,"Peak24Hours":41,"AllTimePeak":76,"AllTimePeakDescription":"2 Months Ago"}"#,
            ))
            .mount(&server)
            .await;
        Mock::given(path("/api/OnlineStatistic/getChart"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"[{"Label":"06 Nov","Value":33},{"Label":"12 Nov","Value":41}]"#,
            ))
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path(), &server.uri());
        let o = s.online_stats().await.unwrap();
        assert_eq!((o.current, o.peak_24h, o.all_time_peak), (39, 41, 76));
        assert_eq!(s.online_history(1).await.unwrap()[1].value, 41);
    }

    #[tokio::test]
    async fn login_keeps_only_the_token_then_events_use_it() {
        let server = MockServer::start().await;
        let token = jwt(r#"{"nameid":"42","unique_name":"Zutyo.sh","exp":4102444800}"#);
        Mock::given(method("POST"))
            .and(path("/api/Account/loginLauncher"))
            .and(body_json(serde_json::json!({"Login":"me","Password":"pw"})))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({ "Id": 42, "Token": format!("Bearer {token}") }),
                ),
            )
            .mount(&server)
            .await;
        Mock::given(path("/api/Event/mainPage"))
            .and(header("authorization", format!("Bearer {token}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "UpcomingEvents": [
                    {"Id": 7, "Name": "Velvet & Dragon", "DateTimeStart": "2099-11-12T20:00:00",
                     "DateTimeEnd": "2099-11-12T22:30:00", "Location": "Amphitheater",
                     "Attendees": [{"Id": 1}], "CreatorNickname": "Shadola",
                     "BannerUrl": "https://cdn.example/events/7.png"},
                    {"Id": 3, "Name": "Old", "DateTimeStart": "2000-01-01T00:00:00",
                     "DateTimeEnd": "2000-01-01T01:00:00"}
                ],
                "Next": {"Id": 9, "Name": "World Exploring", "DateTimeStart": "2099-11-11T18:00:00"}
            })))
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path(), &server.uri());
        assert!(s.somnium_account().is_none());
        assert!(s.upcoming_events().await.is_err(), "needs a session");
        let LoginStep::Done(a) = s.somnium_login(" me ", "pw").await.unwrap() else {
            panic!("no second factor on this account")
        };
        assert_eq!(a.name, "Zutyo.sh");
        assert_eq!(s.somnium_account().unwrap().user_id.as_deref(), Some("42"));
        let file = d.path().join(".config/unilux/somnium-session.json");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!std::fs::read_to_string(&file).unwrap().contains("pw"));
        let evs = s.upcoming_events().await.unwrap();
        assert_eq!(
            evs.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["World Exploring", "Velvet & Dragon"],
            "past events dropped, soonest first"
        );
        let v = &evs[1];
        assert_eq!(v.attendees, Some(1));
        assert_eq!(v.location.as_deref(), Some("Amphitheater"));
        assert_eq!(
            v.image_url.as_deref(),
            Some("https://cdn.example/events/7.png")
        );
        s.somnium_logout().unwrap();
        assert!(s.somnium_account().is_none());
    }

    #[tokio::test]
    async fn wrong_password_is_explained() {
        let server = MockServer::start().await;
        Mock::given(body_json(serde_json::json!({"Login":"bad","Password":"x"})))
            .respond_with(ResponseTemplate::new(400).set_body_string(
                r#"{"Status":400,"Title":"Wrong data","Description":"Email/UserName or password is wrong"}"#,
            ))
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path(), &server.uri());
        let e = s.somnium_login("bad", "x").await.unwrap_err().to_string();
        assert!(e.contains("password is wrong"), "{e}");
        assert!(s.somnium_account().is_none());
    }

    #[tokio::test]
    async fn two_factor_code_finishes_the_sign_in() {
        let server = MockServer::start().await;
        let token = jwt(r#"{"nameid":"42","unique_name":"Zutyo.sh"}"#);
        Mock::given(path("/api/Account/loginLauncher"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Token": "temp-token", "IsSMSEnabled": true, "IsAuthenticatorConnected": true,
                "PhoneNumberForTwoFactor": "+33 *** ** 12"
            })))
            .mount(&server)
            .await;
        Mock::given(path("/api/Account/sendMessage2fa"))
            .and(body_json(
                serde_json::json!({"TemporaryToken":"temp-token","Code":"","Type":1}),
            ))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path("/api/Account/verify2fa"))
            .and(body_json(serde_json::json!({"TemporaryToken":"temp-token","Code":"000000","Type":2,"TrustDevice":false})))
            .respond_with(ResponseTemplate::new(400).set_body_string(
                r#"{"Status":400,"Title":"Wrong data","Description":"Code is wrong"}"#,
            ))
            .mount(&server)
            .await;
        Mock::given(path("/api/Account/verify2fa"))
            .and(body_json(serde_json::json!({"TemporaryToken":"temp-token","Code":"123456","Type":2,"TrustDevice":false})))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "Id": 42, "Token": token })))
            .mount(&server)
            .await;
        let d = tempfile::tempdir().unwrap();
        let s = svc(d.path(), &server.uri());
        assert!(
            s.somnium_verify("1", TwoFactorMethod::Authenticator)
                .await
                .is_err(),
            "no sign-in yet"
        );
        let LoginStep::Code(c) = s.somnium_login("me", "pw").await.unwrap() else {
            panic!("a code is needed")
        };
        assert!(c.sms && c.authenticator);
        assert_eq!(c.phone.as_deref(), Some("+33 *** ** 12"));
        assert!(
            s.somnium_account().is_none(),
            "the temporary token is not a session"
        );
        s.somnium_send_sms().await.unwrap();
        let e = s
            .somnium_verify("000000", TwoFactorMethod::Authenticator)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("Code is wrong"), "{e}");
        let a = s
            .somnium_verify("123 456", TwoFactorMethod::Authenticator)
            .await
            .unwrap();
        assert_eq!(a.name, "Zutyo.sh");
        assert_eq!(s.somnium_account().unwrap().name, "Zutyo.sh");
        assert!(
            s.somnium_verify("123456", TwoFactorMethod::Authenticator)
                .await
                .is_err(),
            "used once"
        );
    }
}
