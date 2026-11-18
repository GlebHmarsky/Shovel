#![cfg(windows)]
use shovel_core::{
    authentication::{Kind, Session},
    git::Git,
    process::{self, Cancellation, Lifetime, Request},
};
use std::{
    ffi::OsString,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

#[test]
fn foreground_authentication_preserves_credential_manager_interaction_policy() {
    let fixture = tempfile::tempdir().unwrap();
    let helper = Path::new(env!("CARGO_BIN_EXE_ShovelAskPass"));
    let session = Session::start(
        helper,
        Arc::new(|prompt| {
            let _ = prompt.answer.send(None);
        }),
    )
    .unwrap();
    let cancel = Cancellation::default();
    let mut git = Git::discover(None, &cancel).unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some("NUL".into()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));

    let script = fixture.path().join("credential helper.sh");
    fs::write(
        &script,
        concat!(
            "cat >/dev/null\n",
            "printf '%s\\n' \"${GCM_INTERACTIVE-unset}\" \"$GIT_TERMINAL_PROMPT\" ",
            "\"$GCM_TRACE_SECRETS\" > helper-environment\n",
            "printf 'username=fixture\\npassword=fixture-only\\n'\n",
        ),
    )
    .unwrap();
    let config = format!(
        "credential.helper=!sh '{}'",
        script.to_str().unwrap().replace('\\', "/").replace('\'', "'\\''"),
    );
    let run = |git: &Git, auth: Option<&Session>, expected: Option<OsString>| {
        let lifetime = if auth.is_some() {
            Lifetime::AcceptedCommand
        } else {
            Lifetime::AutomaticFetch
        };
        let environment = git.network_environment(fixture.path(), auth, &cancel).unwrap();
        let mut request = git.request(
            fixture.path(),
            ["-c", &config, "credential", "fill"],
            lifetime,
            16384,
        );
        request.environment.extend(environment);
        request.input = b"protocol=https\nhost=example.invalid\n\n".to_vec();
        let result = process::run(&request, &cancel, None).unwrap();
        assert!(result.success());
        let output = String::from_utf8(result.stdout).unwrap();
        assert!(output.contains("username=fixture\n"));
        let actual = fs::read_to_string(fixture.path().join("helper-environment")).unwrap();
        let expected = expected.as_deref().and_then(|x| x.to_str()).unwrap_or("unset");
        assert_eq!(actual.lines().collect::<Vec<_>>(), [expected, "0", "0"]);
    };
    run(&git, Some(&session), std::env::var_os("GCM_INTERACTIVE"));
    git.environment.insert("GCM_INTERACTIVE".into(), None);
    run(&git, Some(&session), None);
    git.environment.insert("GCM_INTERACTIVE".into(), Some("false".into()));
    run(&git, Some(&session), Some("false".into()));
    git.environment.insert("Gcm_Interactive".into(), Some("true".into()));
    run(&git, None, Some("never".into()));
    assert!(session.failure().is_none());
}

#[test]
fn git_inherits_native_ssh_options_without_inheriting_repository_overrides() {
    use std::{os::windows::process::CommandExt, process::Command};

    const FIXTURE: &str = "SHOVEL_TEST_INHERITED_SSH";
    if std::env::var_os(FIXTURE).is_none() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "git_inherits_native_ssh_options_without_inheriting_repository_overrides"])
            .env(FIXTURE, "1")
            .env("GIT_SSH_COMMAND", "fixture ssh command")
            .env("GIT_SSH_VARIANT", "simple")
            .env("GIT_DIR", "not-a-repository")
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(child.status.success(), "{child:?}");
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let cancel = Cancellation::default();
    let mut git = Git::discover(None, &cancel).unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some("NUL".into()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    let environment = git.network_environment(fixture.path(), None, &cancel).unwrap();
    let mut request = git.request(
        fixture.path(),
        [
            "-c",
            r#"alias.shovel-fixture=!printf '%s\n' "$GIT_SSH_COMMAND" "$GIT_SSH_VARIANT" "${GIT_DIR-unset}""#,
            "shovel-fixture",
        ],
        Lifetime::AcceptedCommand,
        16384,
    );
    request.environment.extend(environment);
    let result = process::run(&request, &cancel, None).unwrap();
    assert!(result.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().lines().collect::<Vec<_>>(),
        ["fixture ssh command", "simple", "unset"],
    );
}

#[test]
fn native_git_credential_fill_invokes_the_rust_helper() {
    let helper = Path::new(env!("CARGO_BIN_EXE_ShovelAskPass"));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let recorded = prompts.clone();
    let session = Session::start(
        helper,
        Arc::new(move |prompt| {
            assert_eq!(prompt.kind, Kind::Secret);
            let username = prompt.message.starts_with("Username");
            recorded.lock().unwrap().push(username);
            prompt
                .answer
                .send(Some(
                    if username {
                        b"fixture-user".to_vec()
                    } else {
                        "secret-שלום".as_bytes().to_vec()
                    }
                    .into(),
                ))
                .unwrap();
        }),
    )
    .unwrap();
    let git = Git::discover(None, &Cancellation::default()).unwrap();
    let root = std::env::temp_dir();
    let mut request = git.request(
        &root,
        ["-c", "credential.helper=", "credential", "fill"],
        Lifetime::AcceptedCommand,
        16384,
    );
    request.environment.extend(session.environment.clone());
    request
        .environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some("NUL".into()));
    request
        .environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    request.input = b"protocol=https\nhost=example.invalid\n\n".to_vec();
    let result = process::run(&request, &Cancellation::default(), None).unwrap();
    assert!(result.success()); // Never include credential output in assertion failures.
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("username=fixture-user\n"));
    assert!(output.contains("password=secret-שלום\n"));
    assert_eq!(*prompts.lock().unwrap(), vec![true, false]);
    assert!(session.failure().is_none());
}

#[test]
fn compiled_helper_preserves_ssh_confirmation_and_refusal_exit_codes() {
    let helper = Path::new(env!("CARGO_BIN_EXE_ShovelAskPass"));
    let session = Session::start(
        helper,
        Arc::new(move |prompt| {
            assert_eq!(prompt.kind, Kind::Confirmation);
            let reply = (prompt.message == "Accept fixture host?").then(|| b"yes".to_vec().into());
            prompt.answer.send(reply).unwrap();
        }),
    )
    .unwrap();
    for (question, accepted) in [
        ("Accept fixture host?", true),
        ("Refuse fixture host?", false),
    ] {
        let mut request = Request::new(helper.into(), std::env::temp_dir(), [question]);
        request.environment = session.environment.clone();
        request
            .environment
            .insert("SSH_ASKPASS_PROMPT".into(), Some("confirm".into()));
        let result = process::run(&request, &Cancellation::default(), None).unwrap();
        assert_eq!(result.success(), accepted);
        assert_eq!(
            result.stdout,
            if accepted { b"yes\n".as_slice() } else { b"" }
        );
        assert!(result.stderr.is_empty());
    }
}
