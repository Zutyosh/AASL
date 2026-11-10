use super::*;

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

#[test]
fn versions_parse_with_or_without_v() {
    assert_eq!(
        v("v0.2.1"),
        Version {
            major: 0,
            minor: 2,
            patch: 1,
            pre: None
        }
    );
    assert_eq!(v("1.4.0-beta.3").pre, Some(3));
    assert!(Version::parse("nightly").is_none());
    assert!(Version::parse("v1.2").is_none());
}

#[test]
fn prerelease_sorts_before_release() {
    assert!(v("0.2.0-beta.2") < v("0.2.0"));
    assert!(v("0.2.0-beta.2") > v("0.2.0-beta.1"));
    assert!(v("0.1.9") < v("0.2.0-beta.1"));
}

#[test]
fn current_matches_cargo() {
    assert_eq!(Version::current(), v(env!("CARGO_PKG_VERSION")));
}

const RELEASES: &str = r#"[
  {"tag_name":"v0.3.0-beta.1","prerelease":true,"draft":false,"body":"beta","html_url":"https://g/b","assets":[{"name":"AASL-x86_64.AppImage","browser_download_url":"https://g/b.AppImage","size":10,"digest":"sha256:bb"}]},
  {"tag_name":"v0.2.0","prerelease":false,"draft":false,"body":"Fixes","html_url":"https://g/2","assets":[{"name":"AASL-x86_64.AppImage","browser_download_url":"https://g/2.AppImage","size":10,"digest":"sha256:22"}]},
  {"tag_name":"patches-ge-GE-Proton11-12-4","prerelease":true,"draft":false,"body":"","html_url":"https://g/p","assets":[]},
  {"tag_name":"v0.1.0","prerelease":false,"draft":false,"body":"First","html_url":"https://g/1","assets":[]},
  {"tag_name":"v9.9.9","prerelease":false,"draft":true,"body":"","html_url":"https://g/d","assets":[]}
]"#;

#[test]
fn releases_skip_drafts_and_non_version_tags() {
    let r = parse_releases(RELEASES).unwrap();
    let tags: Vec<&str> = r.iter().map(|r| r.tag.as_str()).collect();
    assert_eq!(tags, ["v0.3.0-beta.1", "v0.2.0", "v0.1.0"]);
    assert_eq!(r[1].assets[0].digest.as_deref(), Some("sha256:22"));
    assert_eq!(r[1].notes, "Fixes");
}

#[test]
fn newest_respects_channel_and_current() {
    let r = parse_releases(RELEASES).unwrap();
    assert_eq!(
        newest(&r, v("0.1.0"), Channel::Stable).unwrap().tag,
        "v0.2.0"
    );
    assert_eq!(
        newest(&r, v("0.1.0"), Channel::Beta).unwrap().tag,
        "v0.3.0-beta.1"
    );
    assert!(newest(&r, v("0.2.0"), Channel::Stable).is_none());
    assert!(newest(&r, v("0.3.0"), Channel::Beta).is_none());
}

#[test]
fn install_kind_detection() {
    let all = |_: &Path| true;
    let none = |_: &Path| false;
    assert_eq!(
        detect_install(
            Some("/home/u/Apps/AASL.AppImage"),
            Path::new("/tmp/.mount_x/usr/bin/aasl"),
            all
        ),
        InstallKind::AppImage {
            path: "/home/u/Apps/AASL.AppImage".into()
        }
    );
    assert_eq!(
        detect_install(None, Path::new("/home/u/.local/bin/aasl"), all),
        InstallKind::Portable {
            dir: "/home/u/.local/bin".into()
        }
    );
    assert_eq!(
        detect_install(None, Path::new("/usr/bin/aasl"), all),
        InstallKind::Managed {
            dir: "/usr/bin".into()
        }
    );
    assert_eq!(
        detect_install(None, Path::new("/opt/aasl/aasl"), none),
        InstallKind::Managed {
            dir: "/opt/aasl".into()
        }
    );
    assert_eq!(
        detect_install(Some(""), Path::new("/home/u/bin/aasl"), all),
        InstallKind::Portable {
            dir: "/home/u/bin".into()
        }
    );
}
