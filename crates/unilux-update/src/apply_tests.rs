use std::os::unix::fs::PermissionsExt;

use sha2::{Digest, Sha256};
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

fn sha(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut b = tar::Builder::new(gz);
    for (name, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        b.append_data(&mut h, name, *data).unwrap();
    }
    b.into_inner().unwrap().finish().unwrap()
}

fn release(server: &MockServer, assets: &[(&str, &[u8], Option<String>)]) -> Release {
    Release {
        tag: "v9.0.0".into(),
        version: Version::parse("9.0.0").unwrap(),
        prerelease: false,
        notes: String::new(),
        url: String::new(),
        assets: assets
            .iter()
            .map(|(n, b, d)| Asset {
                name: n.to_string(),
                url: format!("{}/dl/{n}", server.uri()),
                size: b.len() as u64,
                digest: d.clone(),
            })
            .collect(),
    }
}

async fn serve(server: &MockServer, name: &str, body: Vec<u8>) {
    Mock::given(path(format!("/dl/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

fn updater(server: &MockServer) -> Updater {
    Updater {
        api_base: format!("{}/api", server.uri()),
        ..Updater::new()
    }
}

#[tokio::test]
async fn check_finds_newer_release() {
    let server = MockServer::start().await;
    let body = r#"[{"tag_name":"v9.0.0","prerelease":false,"body":"new","html_url":"u","assets":[]},{"tag_name":"v0.0.1","prerelease":false,"assets":[]}]"#;
    Mock::given(path("/api/repos/Zutyosh/Unilux/releases"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;
    let u = updater(&server);
    let r = u
        .check(Version::parse("0.1.0").unwrap(), Channel::Stable)
        .await
        .unwrap();
    assert_eq!(r.unwrap().tag, "v9.0.0");
    assert!(
        u.check(Version::parse("9.0.0").unwrap(), Channel::Stable)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn appimage_is_replaced_and_backed_up() {
    let server = MockServer::start().await;
    let new = b"new appimage".to_vec();
    serve(&server, APPIMAGE_ASSET, new.clone()).await;
    let d = tempfile::tempdir().unwrap();
    let app = d.path().join("Unilux.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let rel = release(
        &server,
        &[(APPIMAGE_ASSET, &new, Some(format!("sha256:{}", sha(&new))))],
    );
    let out = updater(&server)
        .apply(
            &rel,
            &InstallKind::AppImage { path: app.clone() },
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&app).unwrap(), new);
    assert_eq!(
        std::fs::read(d.path().join("Unilux.AppImage.old")).unwrap(),
        b"old"
    );
    assert_eq!(
        std::fs::metadata(&app).unwrap().permissions().mode() & 0o111,
        0o111
    );
    assert_eq!(out.replaced, vec![app]);
    assert!(out.restart_needed);
}

#[tokio::test]
async fn bad_digest_leaves_the_install_alone() {
    let server = MockServer::start().await;
    serve(&server, APPIMAGE_ASSET, b"tampered".to_vec()).await;
    let d = tempfile::tempdir().unwrap();
    let app = d.path().join("Unilux.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let rel = release(
        &server,
        &[(
            APPIMAGE_ASSET,
            b"tampered",
            Some(format!("sha256:{}", "0".repeat(64))),
        )],
    );
    assert!(
        updater(&server)
            .apply(
                &rel,
                &InstallKind::AppImage { path: app.clone() },
                |_, _| {}
            )
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&app).unwrap(), b"old");
}

#[tokio::test]
async fn unverifiable_asset_is_refused() {
    let server = MockServer::start().await;
    serve(&server, APPIMAGE_ASSET, b"x".to_vec()).await;
    let d = tempfile::tempdir().unwrap();
    let app = d.path().join("Unilux.AppImage");
    std::fs::write(&app, b"old").unwrap();
    let rel = release(&server, &[(APPIMAGE_ASSET, b"x", None)]);
    let err = updater(&server)
        .apply(
            &rel,
            &InstallKind::AppImage { path: app.clone() },
            |_, _| {},
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("checksum"), "{err}");
}

#[tokio::test]
async fn portable_binaries_are_replaced_from_the_tarball() {
    let server = MockServer::start().await;
    let tgz = tarball(&[("unilux", b"cli v9"), ("unilux-gui", b"gui v9")]);
    serve(&server, TARBALL_ASSET, tgz.clone()).await;
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("unilux"), b"cli old").unwrap();
    std::fs::write(d.path().join("unilux-gui"), b"gui old").unwrap();
    let rel = release(
        &server,
        &[(TARBALL_ASSET, &tgz, Some(format!("sha256:{}", sha(&tgz))))],
    );
    let out = updater(&server)
        .apply(
            &rel,
            &InstallKind::Portable {
                dir: d.path().into(),
            },
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(d.path().join("unilux")).unwrap(), b"cli v9");
    assert_eq!(std::fs::read(d.path().join("unilux-gui")).unwrap(), b"gui v9");
    assert_eq!(out.replaced.len(), 2);
}

#[tokio::test]
async fn package_manager_installs_are_not_touched() {
    let server = MockServer::start().await;
    let rel = release(&server, &[]);
    let err = updater(&server)
        .apply(
            &rel,
            &InstallKind::Managed {
                dir: "/usr/bin".into(),
            },
            |_, _| {},
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("package manager"), "{err}");
}
