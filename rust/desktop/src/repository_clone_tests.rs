use super::*;
use std::path::Path;
fn command(h: &Harness, path: &Path, args: &[&str]) {
    let result = shovel_core::process::run(
        &h.app.git.as_ref().unwrap().request(
            path,
            args.iter().copied(),
            shovel_core::process::Lifetime::AcceptedCommand,
            65536,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(result.success(), "{result:?}");
}
fn configure(h: &Harness, key: &str, value: &str) {
    command(
        h,
        h.temp.path(),
        &[
            "config",
            "--file",
            h.temp.path().join("isolated.config").to_str().unwrap(),
            key,
            value,
        ],
    );
}
fn source(h: &Harness) -> PathBuf {
    let source = h.temp.path().join("-source 日本語");
    h.app
        .git
        .as_ref()
        .unwrap()
        .initialize_repository(&Options {
            directory: source.clone(),
            initial_branch: "main".into(),
        })
        .unwrap();
    fs::write(source.join("file.txt"), b"base\n").unwrap();
    command(h, &source, &["add", "."]);
    command(h, &source, &["commit", "-m", "base"]);
    source
}
fn options(h: &mut Harness, source: &str, target: &Path) {
    h.app.operations.set_authentication_helper(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/ShovelRustAskPass.exe"),
    );
    h.click(&text("FileMenu"));
    h.click(&text("ActionTitleClone"));
    let form = h.app.initialization.form.as_mut().unwrap();
    form.source = source.into();
    form.directory = target.to_string_lossy().into_owned();
}
#[test]
fn clone_options_cancel_without_git_or_destination_work() {
    let mut h = Harness::new();
    let target = h.temp.path().join("never-created");
    let mut missing = h.app.git.as_ref().unwrap().as_ref().clone();
    missing.executable = h.temp.path().join("missing-git.exe");
    h.app.git = Some(Arc::new(missing));
    options(&mut h, "https://example.invalid/repo.git", &target);
    h.click(&text("ActionCancel"));
    assert!(!target.exists());
    assert!(h.app.scheduler.records().is_empty());
    assert!(!h.app.initialization.is_open());
}
struct Release(PathBuf);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, b"finish");
    }
}
#[test]
fn clone_finishes_after_tab_closure_and_opens_history_with_native_hook_result() {
    let mut h = Harness::new();
    let source = source(&h);
    h.app.open(source.clone(), &h.ctx);
    h.until(|a| !a.tabs[0].loading);
    let hooks = h.temp.path().join("hooks");
    fs::create_dir(&hooks).unwrap();
    let release = Release(h.temp.path().join("release"));
    let quoted = release
        .0
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''");
    fs::write(hooks.join("post-checkout"), format!("#!/bin/sh\nprintf 'ran\\n' > hook-ran\nwhile ! test -e '{quoted}'; do sleep 0.1; done\n")).unwrap();
    configure(&h, "core.hooksPath", hooks.to_str().unwrap());
    let target = h.temp.path().join("clone");
    options(&mut h, source.to_str().unwrap(), &target);
    h.click(&text("CloneSubmit"));
    h.until(|_| target.join("hook-ran").exists());
    assert!(h.app.scheduler.has_accepted_work());
    h.app.close(&h.ctx);
    assert!(!h.app.closing);
    assert_eq!(h.app.notice, Some(text("AppCommandRunningClose")));
    h.app.close_tab(h.app.tabs[0].id, &h.ctx);
    assert!(h.app.tabs.is_empty());
    assert!(h.app.initialization.busy);
    drop(release);
    h.until(|a| !a.initialization.busy && a.tabs.first().is_some_and(|t| !t.loading));
    assert_eq!(h.app.tabs[h.app.selected].path, target);
    assert_eq!(
        h.app.tabs[h.app.selected].page.as_ref().unwrap().entries[0]
            .commit
            .subject(),
        "base"
    );
    assert_eq!(fs::read(target.join("file.txt")).unwrap(), b"base\n");
    assert_eq!(fs::read(target.join("hook-ran")).unwrap(), b"ran\n");
    assert!(
        h.app
            .operations
            .summary(&target)
            .unwrap()
            .contains(&text("CloneSucceeded"))
    );
}
#[test]
fn clone_native_failure_opens_partial_repository_and_retains_actual_error() {
    let mut h = Harness::new();
    let source = source(&h);
    let hooks = h.temp.path().join("hooks");
    fs::create_dir(&hooks).unwrap();
    fs::write(
        hooks.join("post-checkout"),
        b"#!/bin/sh\nprintf 'ran\\n' > hook-ran\nexit 1\n",
    )
    .unwrap();
    configure(&h, "core.hooksPath", hooks.to_str().unwrap());
    let target = h.temp.path().join("partial");
    options(&mut h, source.to_str().unwrap(), &target);
    h.click(&text("CloneSubmit"));
    h.until(|a| !a.initialization.busy && a.tabs.first().is_some_and(|t| !t.loading));
    assert_eq!(h.app.tabs[h.app.selected].path, target);
    assert_eq!(h.app.notice, Some(text("CloneIncomplete")));
    assert!(
        h.app
            .operations
            .summary(&target)
            .unwrap()
            .contains(&text("CloneIncomplete"))
    );
    assert_eq!(fs::read(target.join("hook-ran")).unwrap(), b"ran\n");
    assert_eq!(fs::read(target.join("file.txt")).unwrap(), b"base\n");
}

struct HttpChallenge {
    address: std::net::SocketAddr,
    stop: Arc<std::sync::atomic::AtomicBool>,
    authenticated: Arc<std::sync::atomic::AtomicUsize>,
    worker: Option<thread::JoinHandle<()>>,
}
impl HttpChallenge {
    fn new() -> Self {
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let authenticated = Arc::new(AtomicUsize::new(0));
        let stopped = stop.clone();
        let count = authenticated.clone();
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(stream) => stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(_) => return,
                };
                stream
                    .set_read_timeout(Some(Duration::from_millis(250)))
                    .unwrap();
                let mut request = Vec::new();
                let mut bytes = [0; 1024];
                while request.len() < 16384 && !request.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&bytes[..n]),
                    }
                }
                let has_auth = String::from_utf8_lossy(&request)
                    .to_ascii_lowercase()
                    .contains("\r\nauthorization: basic ");
                let response = if has_auth {
                    count.fetch_add(1, Ordering::AcqRel);
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .as_slice()
                } else {
                    b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"fixture\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_slice()
                };
                // Synthetic credential bytes remain transient and are never logged.
                let _ = stream.write_all(response);
            }
        });
        Self {
            address,
            stop,
            authenticated,
            worker: Some(worker),
        }
    }
}
impl Drop for HttpChallenge {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
#[test]
fn native_clone_uses_rust_authentication_ui_and_decline_does_not_cancel_the_process() {
    for decline in [false, true] {
        let server = HttpChallenge::new();
        let mut h = Harness::new();
        let target = h.temp.path().join("authenticated");
        options(
            &mut h,
            &format!("http://{}/fixture.git", server.address),
            &target,
        );
        h.click(&text("CloneSubmit"));
        h.until(|a| a.operations.modal_open());
        assert!(h.app.scheduler.has_accepted_work());
        h.app.close(&h.ctx);
        assert!(!h.app.closing);
        if decline {
            h.click(&text("RustAuthDecline"));
        } else {
            for reply in ["fixture-user", "fixture-password"] {
                h.until(|a| a.operations.modal_open());
                h.render(vec![egui::Event::Text(reply.into())])
                    .drop_without_applying_deltas();
                h.click(&text("AuthSubmit"));
            }
        }
        h.until(|a| !a.initialization.busy && !a.operations.modal_open());
        assert!(h.app.tabs.is_empty());
        assert_eq!(h.app.notice, Some(text("CloneIncomplete")));
        assert_eq!(
            server
                .authenticated
                .load(std::sync::atomic::Ordering::Acquire)
                > 0,
            !decline
        );
        assert!(
            !h.app
                .initialization
                .capture
                .text
                .contains("fixture-password")
        );
    }
}
