#![cfg(windows)]
use shovel_core::{
    authentication::{Kind, Session},
    git::Git,
    process::{self, Cancellation, Lifetime, Request},
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[test]
fn native_git_credential_fill_invokes_the_rust_helper() {
    let helper = Path::new(env!("CARGO_BIN_EXE_ShovelRustAskPass"));
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
    let helper = Path::new(env!("CARGO_BIN_EXE_ShovelRustAskPass"));
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
