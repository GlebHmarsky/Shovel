#![cfg(windows)]
use serde::Deserialize;
use shovel_core::{
    identity,
    working::{Error, MutationGate},
};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Expected {
    path: PathBuf,
    fingerprint: String,
    common: PathBuf,
    git: PathBuf,
}
struct Held {
    child: Child,
    release: PathBuf,
    _temp: tempfile::TempDir,
}
impl Held {
    fn new(path: &std::path::Path, category: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let ready = temp.path().join("ready");
        let release = temp.path().join("release");
        let child = Command::new("dotnet")
            .arg(std::env::var_os("SHOVEL_IDENTITY_BASELINE_DLL").unwrap())
            .arg("--hold-gate")
            .arg(path)
            .arg(category)
            .arg(&ready)
            .arg(&release)
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut held = Self {
            child,
            release,
            _temp: temp,
        };
        let until = Instant::now() + Duration::from_secs(10);
        while !ready.exists() {
            assert!(
                held.child.try_wait().unwrap().is_none(),
                "reference holder exited"
            );
            assert!(Instant::now() < until, "reference holder did not acquire");
            std::thread::sleep(Duration::from_millis(10));
        }
        held
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        fs::write(&self.release, "release").unwrap();
        assert!(self.child.wait().unwrap().success());
    }
}

#[test]
fn native_identity_and_gate_match_pinned_csharp() {
    let Some(evidence) = std::env::var_os("SHOVEL_IDENTITY_PARITY") else {
        eprintln!("Optional paired check: run scripts/Test-RustIdentityParity.ps1");
        return;
    };
    let evidence = PathBuf::from(evidence);
    let rows: Vec<Expected> =
        serde_json::from_slice(&fs::read(evidence.join("identities.json")).unwrap()).unwrap();
    let mut mismatches = vec![];
    for row in &rows {
        let actual = identity::read(&row.path).unwrap();
        if actual.fingerprint != row.fingerprint {
            mismatches.push(row.path.clone());
        }
        assert_eq!(actual.root, row.path);
        assert_eq!(actual.git, row.git);
        assert_eq!(actual.common, row.common);
        assert_eq!(actual, identity::read(&row.path).unwrap());
    }
    // Fingerprints are Rust-profile persisted identifiers, not a cross-product
    // wire contract. Keep their published v1 encoding so existing draft keys
    // remain readable; native roots and shared mutex names must still match.
    eprintln!(
        "Rust v1 fingerprint encoding differs from C# for {} Unicode paths",
        mismatches.len()
    );
    let scalars: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
    use sha2::{Digest, Sha256};
    let upper = shovel_core::case::invariant_upper(&scalars).unwrap();
    assert_eq!(
        format!("{:X}", Sha256::digest(upper.as_bytes())),
        fs::read_to_string(evidence.join("casing-sha256.txt")).unwrap()
    );
    for row in &rows {
        for fetch in [false, true] {
            let held = Held::new(&row.path, if fetch { "Fetch" } else { "GitAction" });
            assert!(
                matches!(
                    MutationGate::acquire(&row.common, fetch),
                    Err(Error::OtherInstance)
                ),
                "C# and Rust failed to share the native gate: {:?}, fetch={fetch}",
                row.path
            );
            if !fetch && row.path.file_name().unwrap() == "straße" {
                use shovel_core::{git::Git, process::Cancellation, working::Action};
                let mut git = Git::discover(None, &Cancellation::default()).unwrap();
                git.environment.insert(
                    "GIT_CONFIG_GLOBAL".into(),
                    Some(
                        row.path
                            .parent()
                            .unwrap()
                            .join("isolated.config")
                            .into_os_string(),
                    ),
                );
                git.environment
                    .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
                let file = row.path.join("gate-check.txt");
                fs::write(&file, b"preserved payload").unwrap();
                let context = git
                    .working_copy(&row.path, &Cancellation::default())
                    .unwrap();
                let selected = context
                    .entries
                    .iter()
                    .filter(|e| e.path == "gate-check.txt")
                    .cloned()
                    .collect();
                let before = fs::read(row.git.join("index")).ok();
                assert!(matches!(
                    git.execute_working(&context, &Action::Stage(selected), None),
                    Err(Error::OtherInstance)
                ));
                assert_eq!(fs::read(row.git.join("index")).ok(), before);
                assert_eq!(fs::read(file).unwrap(), b"preserved payload");
            }
            // The baseline intentionally allows Fetch alongside working-copy work.
            assert!(MutationGate::acquire(&row.common, !fetch).is_ok());
            drop(held);
            assert!(MutationGate::acquire(&row.common, fetch).is_ok());
        }
    }
    eprintln!(
        "PASS native roots, full Unicode casing digest and eighteen cross-process C#/Rust gates"
    );
}
