//! End-to-end install against a local HTTP server and a fake umu.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sha2::{Digest, Sha256};
use unilux_components::sources::LAUNCHER_REL_PATH;
use unilux_components::{CpuLevel, Downloader, Sources};
use unilux_core::exec::{BoxFuture, CommandSpec, Exec};
use unilux_core::{Config, Events, Paths, Result};
use unilux_install::{Installer, Mode, Options};
use unilux_system::FakeHost;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
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

fn plain_tar(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    for (name, data) in files {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        b.append_data(&mut h, name, *data).unwrap();
    }
    b.into_inner().unwrap()
}

/// Pretends to be umu-run: wineboot creates system.reg, the Somnium
/// installer creates the launcher.
struct FakeUmu;

impl Exec for FakeUmu {
    fn run<'a>(
        &'a self,
        spec: &'a CommandSpec,
        _: &'a Events,
    ) -> BoxFuture<'a, Result<Option<i32>>> {
        Box::pin(async move {
            let prefix = Path::new(&spec.env["WINEPREFIX"]).to_path_buf();
            assert!(spec.env["PROTONPATH"].ends_with("proton/ge/GE-Proton11-5"));
            match spec.args.first().map(String::as_str) {
                Some("wineboot") => std::fs::write(prefix.join("system.reg"), "").unwrap(),
                Some(exe) if exe.ends_with(".exe") => {
                    assert!(spec.args.iter().any(|a| a == "/VERYSILENT"));
                    assert!(spec.args.iter().any(|a| a.starts_with("/LANG=")));
                    let l = prefix.join(LAUNCHER_REL_PATH);
                    std::fs::create_dir_all(l.parent().unwrap()).unwrap();
                    std::fs::write(l, "MZ").unwrap();
                }
                other => panic!("unexpected {other:?}"),
            }
            Ok(Some(0))
        })
    }
}

async fn server() -> MockServer {
    let s = MockServer::start().await;
    let ok = |b: Vec<u8>| ResponseTemplate::new(200).set_body_bytes(b);
    let umu = plain_tar(&[("umu/umu-run", b"#!/bin/sh\n")]);
    let proton = tar_gz(&[
        ("GE-Proton11-5/proton", b"py"),
        (
            "GE-Proton11-5/files/lib/wine/x86_64-windows/secur32.dll",
            b"stock",
        ),
    ]);
    let bundle = tar_gz(&[
        ("archive-format.txt", b"2\n"),
        ("x86_64-windows/secur32.dll", b"patched"),
    ]);
    let uri = s.uri();
    let releases = format!(
        r#"[{{"tag_name":"GE-Proton11-5","assets":[{{"name":"GE-Proton11-5-x86_64.tar.gz","browser_download_url":"{uri}/dl/ge.tar.gz","size":{},"digest":"sha256:{}"}}]}}]"#,
        proton.len(),
        hex::encode(Sha256::digest(&proton))
    );
    let catalog = format!(
        "schema = 1\nrecommended_proton = \"ge:GE-Proton11-5\"\n[[bundle]]\nid = \"b1\"\ntitle = \"t\"\narchive_format = 2\nurl = \"{uri}/dl/bundle.tar.gz\"\nsha256 = \"{}\"\ntargets = [{{ proton = \"ge:GE-Proton11-5\", status = \"verified\" }}]\n",
        hex::encode(Sha256::digest(&bundle))
    );
    for (p, body) in [
        ("/catalog.toml", catalog.into_bytes()),
        (
            "/api/repos/GloriousEggroll/proton-ge-custom/releases",
            releases.into_bytes(),
        ),
        ("/umu/1.4.4/umu-launcher-1.4.4-zipapp.tar", umu),
        ("/dl/ge.tar.gz", proton),
        ("/dl/bundle.tar.gz", bundle),
        ("/somnium.exe", b"MZ fake installer".to_vec()),
    ] {
        Mock::given(path(p)).respond_with(ok(body)).mount(&s).await;
    }
    s
}

fn installer(home: &Path, uri: &str, umu_sha: String) -> Installer {
    let h = home.display().to_string();
    let paths = Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
    let config = Config {
        catalog_url: format!("{uri}/catalog.toml"),
        ..Default::default()
    };
    let host = FakeHost::new()
        .with_command("python3 --version", "Python 3.12.3")
        .with_file("/usr/share/vulkan/icd.d/x.json", "{}");
    Installer {
        paths,
        config,
        sources: Sources {
            umu_base: format!("{uri}/umu"),
            umu_version: "1.4.4".into(),
            umu_sha256: umu_sha,
            github_api: format!("{uri}/api"),
            somnium_installer_url: format!("{uri}/somnium.exe"),
            somnium_api: format!("{uri}/somnium"),
        },
        host: Arc::new(host),
        exec: Arc::new(FakeUmu),
        events: unilux_core::event_bus(),
        downloader: Downloader::new(),
        cancel: Arc::new(AtomicBool::new(false)),
        cpu: CpuLevel::X86_64,
    }
}

#[tokio::test]
async fn install_then_repair_then_uninstall() {
    let s = server().await;
    let umu_sha = hex::encode(Sha256::digest(plain_tar(&[(
        "umu/umu-run",
        b"#!/bin/sh\n",
    )])));
    let home = tempfile::tempdir().unwrap();
    let inst = installer(home.path(), &s.uri(), umu_sha);
    let exe = home.path().join("SomniumSetup.exe");
    std::fs::write(&exe, "MZ").unwrap();

    // No installer given: Unilux downloads it and runs it silently.
    let st = inst.run(Mode::Install, &Options::default()).await.unwrap();
    assert!(st.is_installed());
    assert!(
        inst.paths
            .downloads_dir()
            .join("SomniumSpaceSetup.exe")
            .exists()
    );

    let opts = Options {
        somnium_installer: Some(exe),
        ..Default::default()
    };
    let st = inst.run(Mode::Install, &opts).await.unwrap();
    assert!(st.is_installed());
    assert_eq!(st.proton.as_deref(), Some("ge:GE-Proton11-5"));
    assert_eq!(st.patch_bundle.as_deref(), Some("b1"));
    let proton = inst.paths.proton_root().join("ge/GE-Proton11-5");
    let dll = proton.join("files/lib/wine/x86_64-windows/secur32.dll");
    assert!(unilux_patches::bundle::status(&proton).intact);
    assert_eq!(std::fs::read_to_string(&dll).unwrap(), "patched");
    assert!(inst.paths.desktop_entry().exists());
    assert!(inst.prefix().join(LAUNCHER_REL_PATH).exists());

    std::fs::write(&dll, "stock again").unwrap();
    inst.run(Mode::Repair, &Options::default()).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dll).unwrap(), "patched");

    let removed = inst.uninstall(false).unwrap();
    assert!(!removed.is_empty());
    assert!(
        inst.prefix().join(LAUNCHER_REL_PATH).exists(),
        "prefix kept"
    );
    assert!(!inst.paths.components_dir().exists());
}

#[tokio::test]
async fn bad_umu_checksum_stops_install() {
    let s = server().await;
    let home = tempfile::tempdir().unwrap();
    let inst = installer(home.path(), &s.uri(), "00".repeat(32));
    let err = inst
        .run(Mode::Install, &Options::default())
        .await
        .unwrap_err();
    assert!(matches!(err, unilux_core::Error::Checksum { .. }), "{err}");
}

#[tokio::test]
async fn repair_without_install_explains() {
    let s = server().await;
    let home = tempfile::tempdir().unwrap();
    let inst = installer(home.path(), &s.uri(), String::new());
    let err = inst
        .run(Mode::Repair, &Options::default())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("drive_c"));
}
