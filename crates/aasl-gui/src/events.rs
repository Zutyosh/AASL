//! Somnium Space's live side on the main screen: players online, the next
//! events, and the optional sign-in that unlocks them.

use aasl_service::{Account, LoginStep, TwoFactor, TwoFactorMethod, UpcomingEvent};
use chrono::{Datelike, Local, TimeZone, Timelike};
use freya::prelude::*;

use crate::i18n::{Lang, current, fill, tr};
use crate::state::{Screen, Section, Ui};
use crate::svc;
use crate::theme::{RADIUS_CONTROL, RADIUS_PILL, T, alpha, glass, size, space};
use crate::widgets::{Tone, body, dot, glass_panel, link, muted, pill, title};

const CARD_W: f32 = 420.;

/// Top bar: how many people are in Somnium right now.
pub fn online_badge(ui: Ui) -> Option<Element> {
    let o = ui.online.read().clone()?;
    Some(
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(space::S)
            .padding((6., 14.))
            .corner_radius(RADIUS_PILL)
            .background(glass())
            .child(dot(if o.current > 0 {
                Tone::Ok
            } else {
                Tone::Neutral
            }))
            .child(
                label()
                    .text(fill(
                        tr("{} online", "{} en ligne"),
                        &[o.current.to_string()],
                    ))
                    .font_size(size::SMALL)
                    .font_weight(FontWeight::MEDIUM)
                    .color(T.text),
            )
            .into_element(),
    )
}

/// Right side of the main screen: the next event large, two more below.
/// Signed out, a short invitation to sign in instead.
pub fn events_panel(mut ui: Ui) -> Element {
    if ui.account.read().is_none() {
        return glass_panel()
            .width(Size::px(CARD_W))
            .spacing(space::S)
            .padding(space::XL)
            .child(title(tr("Somnium events", "Événements Somnium")).font_size(size::LEAD))
            .child(muted(tr(
                "Sign in with your Somnium Space account to see what is on next.",
                "Connectez votre compte Somnium Space pour voir les prochains événements.",
            )))
            .child(
                rect().padding((space::S, 0., 0., 0.)).child(
                    Button::new()
                        .corner_radius(RADIUS_PILL)
                        .on_press(move |_| {
                            ui.section.set(Section::General);
                            ui.screen.set(Screen::Settings)
                        })
                        .child(tr("Sign in", "Se connecter")),
                ),
            )
            .into_element();
    }
    let events = ui.events.read().clone();
    let Some(((next, img), rest)) = events.split_first() else {
        return glass_panel()
            .width(Size::px(CARD_W))
            .padding(space::XL)
            .child(muted(tr(
                "No event planned for now.",
                "Aucun événement prévu pour l'instant.",
            )))
            .into_element();
    };
    glass_panel()
        .width(Size::px(CARD_W))
        .padding(0.)
        .overflow(Overflow::Clip)
        .child(banner(img.clone()))
        .child(
            rect()
                .width(Size::fill())
                .padding((space::L, space::XL, space::XL, space::XL))
                .spacing(space::S)
                .child(
                    rect()
                        .width(Size::fill())
                        .horizontal()
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .spacing(space::M)
                        .child(
                            title(next.name.clone())
                                .font_size(size::LEAD)
                                .max_lines(2)
                                .width(Size::flex(1.)),
                        )
                        .child(countdown(next, true)),
                )
                .child(body(when(next)).color(alpha(T.text, 0.85)))
                .maybe_child(meta(next).map(muted))
                .maybe_child(next.description.clone().map(|d| {
                    muted(d.replace(['\r', '\n'], " "))
                        .max_lines(3)
                        .text_overflow(TextOverflow::Ellipsis)
                }))
                .children(
                    rest.iter()
                        .take(2)
                        .map(|(ev, _)| later(ev).key(ev.id.clone()).into_element())
                        .collect::<Vec<_>>(),
                ),
        )
        .into_element()
}

fn banner(img: Option<std::path::PathBuf>) -> Element {
    let h = CARD_W * 9. / 16.;
    match img {
        Some(p) => ImageViewer::new(p)
            .width(Size::fill())
            .height(Size::px(h))
            .aspect_ratio(AspectRatio::Max)
            .into_element(),
        None => rect().into_element(),
    }
}

/// A later event: one line with its name, its date and its countdown.
fn later(ev: &UpcomingEvent) -> Rect {
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .cross_align(Alignment::Center)
        .spacing(space::M)
        .padding((space::M, 0., 0., 0.))
        .border(Border::new().fill(alpha(T.text, 0.08)).width(BorderWidth {
            top: 1.,
            ..Default::default()
        }))
        .child(
            rect()
                .width(Size::flex(1.))
                .spacing(2.)
                .child(
                    body(ev.name.clone())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .child(muted(when(ev))),
        )
        .child(countdown(ev, false))
}

/// Time until the event; only the next one carries the accent.
fn countdown(ev: &UpcomingEvent, next: bool) -> Rect {
    let now = Local::now().timestamp().max(0) as u64;
    let (text, tone) = if ev.start <= now {
        (tr("On now", "En cours").to_string(), Tone::Ok)
    } else {
        let s = ev.start - now;
        let (d, h, m) = (s / 86400, s % 86400 / 3600, s % 3600 / 60);
        let t = match (d, current()) {
            (0, _) if h == 0 => format!("{m} min"),
            (0, _) => format!("{h} h {m:02}"),
            (d, Lang::Fr) => format!("{d} j {h} h"),
            (d, Lang::En) => format!("{d} d {h} h"),
        };
        (
            fill(tr("in {}", "dans {}"), &[t]),
            if next { Tone::Accent } else { Tone::Neutral },
        )
    };
    rect()
        .padding((3., 10.))
        .corner_radius(RADIUS_PILL)
        .background(alpha(tone.color(), 0.14))
        .child(
            label()
                .text(text)
                .font_size(size::SMALL)
                .font_weight(FontWeight::MEDIUM)
                .color(tone.color()),
        )
}

/// "Thu 12 Nov, 20:00 to 22:30" in local time.
fn when(ev: &UpcomingEvent) -> String {
    let at = |s: u64| Local.timestamp_opt(s as i64, 0).single();
    let Some(start) = at(ev.start) else {
        return String::new();
    };
    let fr = current() == Lang::Fr;
    let days_en = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let days_fr = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
    let months_en = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let months_fr = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ];
    let wd = start.weekday().num_days_from_monday() as usize;
    let mo = start.month0() as usize;
    let hm = |t: chrono::DateTime<Local>| format!("{:02}:{:02}", t.hour(), t.minute());
    let mut s = if fr {
        format!(
            "{} {} {}, {}",
            days_fr[wd],
            start.day(),
            months_fr[mo],
            hm(start)
        )
    } else {
        format!(
            "{} {} {}, {}",
            days_en[wd],
            start.day(),
            months_en[mo],
            hm(start)
        )
    };
    if let Some(end) = ev.end.and_then(at) {
        s.push_str(tr(" to ", " à "));
        s.push_str(&hm(end));
    }
    s
}

/// Where, who hosts, how many are going: whichever the event says.
fn meta(ev: &UpcomingEvent) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(l) = &ev.location {
        parts.push(l.clone());
    }
    if let Some(c) = &ev.creator {
        parts.push(fill(
            tr("hosted by {}", "organisé par {}"),
            std::slice::from_ref(c),
        ));
    }
    if let Some(n) = ev.attendees.filter(|n| *n > 0) {
        parts.push(fill(tr("{} going", "{} inscrits"), &[n.to_string()]));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

#[cfg(test)]
thread_local! {
    /// Snapshot test: open the form on the code step.
    pub static TEST_CHALLENGE: std::cell::RefCell<Option<TwoFactor>> = const { std::cell::RefCell::new(None) };
}

fn test_challenge() -> Option<TwoFactor> {
    #[cfg(test)]
    {
        return TEST_CHALLENGE.with(|c| c.borrow().clone());
    }
    #[allow(unreachable_code)]
    None
}

/// Sign in to Somnium Space, or the signed-in account with a way out.
#[derive(PartialEq)]
pub struct AccountForm(pub Ui);

impl Component for AccountForm {
    fn render(&self) -> impl IntoElement {
        let mut ui = self.0;
        let login = use_state(String::new);
        let mut password = use_state(String::new);
        let mut code = use_state(String::new);
        let mut challenge = use_state(test_challenge);
        let mut method = use_state(|| TwoFactorMethod::Authenticator);
        let mut sms_sent = use_state(|| false);
        let mut busy = use_state(|| false);
        let mut error = use_state(|| None::<String>);
        let mut signed_in = move |a: Account| {
            password.set(String::new());
            code.set(String::new());
            challenge.set(None);
            ui.account.set(Some(a));
            ui.refresh_events();
        };
        let mut submit = move || {
            let (l, p) = (login.peek().clone(), password.peek().clone());
            if *busy.peek() || l.trim().is_empty() || p.is_empty() {
                return;
            }
            busy.set(true);
            error.set(None);
            spawn_in_window(async move {
                let r = tokio::spawn(async move { svc().somnium_login(&l, &p).await }).await;
                busy.set(false);
                match r {
                    Ok(Ok(LoginStep::Done(a))) => signed_in(a),
                    Ok(Ok(LoginStep::Code(c))) => {
                        password.set(String::new());
                        method.set(if c.authenticator {
                            TwoFactorMethod::Authenticator
                        } else {
                            TwoFactorMethod::Sms
                        });
                        sms_sent.set(false);
                        challenge.set(Some(c));
                    }
                    Ok(Err(e)) => error.set(Some(e.to_string())),
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
        };
        let mut verify = move || {
            let (c, m) = (code.peek().clone(), *method.peek());
            if *busy.peek() || c.trim().is_empty() {
                return;
            }
            busy.set(true);
            error.set(None);
            spawn_in_window(async move {
                let r = tokio::spawn(async move { svc().somnium_verify(&c, m).await }).await;
                busy.set(false);
                match r {
                    Ok(Ok(a)) => signed_in(a),
                    Ok(Err(e)) => {
                        code.set(String::new());
                        error.set(Some(e.to_string()))
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
        };
        let mut send_sms = move || {
            if *busy.peek() {
                return;
            }
            busy.set(true);
            error.set(None);
            spawn_in_window(async move {
                let r = tokio::spawn(async { svc().somnium_send_sms().await }).await;
                busy.set(false);
                match r {
                    Ok(Ok(())) => sms_sent.set(true),
                    Ok(Err(e)) => error.set(Some(e.to_string())),
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
        };
        if let Some(a) = ui.account.read().clone() {
            return rect()
                .width(Size::fill())
                .horizontal()
                .content(Content::Flex)
                .cross_align(Alignment::Center)
                .spacing(space::M)
                .padding((space::M, space::L))
                .child(dot(Tone::Ok))
                .child(
                    rect()
                        .width(Size::flex(1.))
                        .spacing(2.)
                        .child(body(a.name))
                        .child(muted(tr(
                            "Signed in. AASL keeps only the session, readable by you alone.",
                            "Connecté. AASL ne garde que la session, lisible par vous seul.",
                        ))),
                )
                .child(
                    Button::new()
                        .compact()
                        .on_press(move |_| match svc().somnium_logout() {
                            Ok(()) => {
                                ui.account.set(None);
                                ui.events.set(Vec::new());
                            }
                            Err(e) => ui.error.set(Some(e.to_string())),
                        })
                        .child(tr("Sign out", "Se déconnecter")),
                )
                .into_element();
        }
        let is_busy = *busy.read();
        let err = error.read().clone().map(|e| body(e).color(T.error));
        if let Some(c) = challenge.read().clone() {
            let m = *method.read();
            let hint = match m {
                TwoFactorMethod::Authenticator => tr(
                    "Enter the 6-digit code from your authenticator app.",
                    "Saisissez le code à 6 chiffres de votre application d'authentification.",
                )
                .to_string(),
                TwoFactorMethod::Sms => match (&c.phone, *sms_sent.read()) {
                    (Some(p), true) => fill(
                        tr("Code sent to {}.", "Code envoyé au {}."),
                        std::slice::from_ref(p),
                    ),
                    (_, true) => tr("Code sent by SMS.", "Code envoyé par SMS.").to_string(),
                    (Some(p), false) => fill(
                        tr("Get a code by SMS at {}.", "Recevez un code par SMS au {}."),
                        std::slice::from_ref(p),
                    ),
                    (None, false) => {
                        tr("Get a code by SMS.", "Recevez un code par SMS.").to_string()
                    }
                },
                TwoFactorMethod::BackupCode => tr(
                    "Enter one of the backup codes Somnium gave you.",
                    "Saisissez l'un des codes de secours fournis par Somnium.",
                )
                .to_string(),
            };
            let mut options = Vec::new();
            if c.authenticator {
                options.push((TwoFactorMethod::Authenticator, tr("App", "Application")));
            }
            if c.sms {
                options.push((TwoFactorMethod::Sms, "SMS"));
            }
            options.push((
                TwoFactorMethod::BackupCode,
                tr("Backup code", "Code de secours"),
            ));
            let ready = !code.read().trim().is_empty() && !is_busy;
            return rect()
                .width(Size::fill())
                .spacing(space::S)
                .padding((space::M, space::L))
                .child(
                    body(tr("Verification code", "Code de vérification"))
                        .font_weight(FontWeight::SEMI_BOLD),
                )
                .child(
                    rect().horizontal().spacing(space::XS).children(
                        options
                            .into_iter()
                            .map(|(opt, name)| {
                                let b = Button::new().compact().corner_radius(RADIUS_PILL);
                                let b = if opt == m { b.filled() } else { b.flat() };
                                b.key(name)
                                    .on_press(move |_| {
                                        method.set(opt);
                                        error.set(None)
                                    })
                                    .child(name)
                                    .into_element()
                            })
                            .collect::<Vec<_>>(),
                    ),
                )
                .child(muted(hint))
                .maybe_child((m == TwoFactorMethod::Sms).then(|| {
                    Button::new()
                        .compact()
                        .corner_radius(RADIUS_PILL)
                        .enabled(!is_busy)
                        .on_press(move |_| send_sms())
                        .child(if *sms_sent.read() {
                            tr("Send again", "Renvoyer")
                        } else {
                            tr("Send the SMS", "Envoyer le SMS")
                        })
                }))
                .child(
                    Input::new(code)
                        .placeholder(tr("Code", "Code"))
                        .auto_focus(true)
                        .width(Size::fill())
                        .on_submit(move |_| verify()),
                )
                .maybe_child(err)
                .child(
                    rect()
                        .width(Size::fill())
                        .horizontal()
                        .cross_align(Alignment::Center)
                        .spacing(space::M)
                        .padding((space::XS, 0., 0., 0.))
                        .child(
                            pill(if is_busy {
                                tr("Checking", "Vérification")
                            } else {
                                tr("Confirm", "Valider")
                            })
                            .corner_radius(RADIUS_PILL)
                            .padding((10., 28.))
                            .enabled(ready)
                            .on_press(move |_| verify()),
                        )
                        .child(link(tr("Cancel", "Annuler")).on_press(move |_| {
                            challenge.set(None);
                            code.set(String::new());
                            error.set(None)
                        })),
                )
                .into_element();
        }
        let field = |s: State<String>, ph: &'static str, hidden: bool| {
            let mut i = Input::new(s).placeholder(ph).width(Size::fill());
            if hidden {
                i = i.mode(InputMode::new_password());
            }
            i.on_submit(move |_| submit())
        };
        let ready = !login.read().trim().is_empty() && !password.read().is_empty();
        rect()
            .width(Size::fill())
            .spacing(space::S)
            .padding((space::M, space::L))
            .child(field(login, tr("E-mail or user name", "E-mail ou nom d'utilisateur"), false))
            .child(field(password, tr("Password", "Mot de passe"), true))
            .maybe_child(err)
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(space::M)
                    .padding((space::XS, 0., 0., 0.))
                    .child(
                        pill(if is_busy {
                            tr("Signing in", "Connexion")
                        } else {
                            tr("Sign in", "Se connecter")
                        })
                        .corner_radius(RADIUS_PILL)
                        .padding((10., 28.))
                        .enabled(ready && !is_busy)
                        .on_press(move |_| submit()),
                    ),
            )
            .child(
                muted(tr(
                    "Optional. It unlocks the events on the main screen. Your password goes to Somnium Space only and is never saved.",
                    "Facultatif. Cela affiche les événements sur l'écran principal. Votre mot de passe n'est envoyé qu'à Somnium Space et n'est jamais enregistré.",
                ))
                .color(alpha(T.text, 0.6)),
            )
            .corner_radius(RADIUS_CONTROL)
            .into_element()
    }
}
