use super::*;
use shovel_core::process::{self, Lifetime};
use std::{
    fs,
    time::{Duration, Instant},
};
struct Harness {
    temp: tempfile::TempDir,
    ctx: egui::Context,
    view: Inspector,
}
impl Harness {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config,"[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        let cmd = |args: &[&str]| {
            let r = process::run(
                &git.request(
                    &root,
                    args.iter().copied(),
                    Lifetime::AcceptedCommand,
                    65536,
                ),
                &Cancellation::default(),
                None,
            )
            .unwrap();
            assert!(r.success(), "{r:?}");
        };
        cmd(&["init", "--initial-branch=main"]);
        fs::write(root.join("a-other.txt"), "other base\n").unwrap();
        fs::write(root.join("tracked.txt"), "original line\n").unwrap();
        cmd(&["add", "a-other.txt", "tracked.txt"]);
        cmd(&["commit", "-m", "root"]);
        fs::write(root.join("a-other.txt"), "unrelated commit content\n").unwrap();
        fs::write(root.join("tracked.txt"), "intended committed content\n").unwrap();
        cmd(&["commit", "-am", "change both files"]);
        fs::write(root.join("tracked.txt"), "uncommitted edit\n").unwrap();
        let context = Arc::new(git.working_copy(&root, &Cancellation::default()).unwrap());
        let entry = context
            .entries
            .iter()
            .find(|e| e.path == "tracked.txt")
            .unwrap()
            .clone();
        let view = Inspector::new(context, entry, Arc::new(git), false);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_embed_viewports(true);
        Self { temp, ctx, view }
    }
    fn render(&mut self, events: Vec<egui::Event>, size: egui::Vec2) -> egui::FullOutput {
        let ctx = self.ctx.clone();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |_| self.view.show(&ctx),
        );
        out.textures_delta.clear();
        out
    }
    fn until(&mut self, ready: impl Fn(&Inspector) -> bool) {
        let start = Instant::now();
        loop {
            self.render(vec![], egui::vec2(1400., 1000.))
                .drop_without_applying_deltas();
            if ready(&self.view) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(25),
                "{}",
                self.view.status
            );
            thread::sleep(Duration::from_millis(15));
        }
    }
    fn click(&mut self, label: &str) {
        let out = self.render(vec![], egui::vec2(1400., 1000.));
        let node = out
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.role() == egui::Role::Button && n.label() == Some(label))
            .unwrap_or_else(|| panic!("missing {label}"));
        let r = node.1.bounds().unwrap();
        let point = egui::pos2((r.x0 + r.x1) as f32 / 2., (r.y0 + r.y1) as f32 / 2.);
        out.drop_without_applying_deltas();
        self.render(
            vec![egui::Event::PointerMoved(point)],
            egui::vec2(1400., 1000.),
        )
        .drop_without_applying_deltas();
        for pressed in [true, false] {
            self.render(
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
                egui::vec2(1400., 1000.),
            )
            .drop_without_applying_deltas();
        }
    }
}
#[test]
fn file_history_opens_after_layout_and_compares_the_requested_file_then_shows_committed_blame() {
    let mut h = Harness::new();
    assert!(!h.view.initialized && !h.view.busy && h.view.page.is_none());
    h.until(|v| v.page.is_some() && !v.busy && !v.comparison.is_loading());
    assert_eq!(h.view.page.as_ref().unwrap().entries.len(), 2);
    let out = h.render(vec![], egui::vec2(1400., 1000.));
    assert!(
        out.platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n
                .value()
                .is_some_and(|s| s.contains("intended committed content")))
    );
    out.drop_without_applying_deltas();
    h.click(&text("FileBlameTab"));
    h.until(|v| !v.busy && !v.blame.is_empty());
    assert_eq!(h.view.blame[0].text, "intended committed content");
    assert!(!h.view.blame.iter().any(|l| l.text.contains("uncommitted")));
    h.render(vec![], egui::vec2(900., 620.))
        .drop_without_applying_deltas();
    h.click(&text("FileHistoryTab"));
    h.until(|v| !v.busy);
    assert_eq!(h.view.page.as_ref().unwrap().entries.len(), 2);
}
#[test]
fn file_history_failed_refresh_retains_rows_and_cancelled_or_closed_generations_cannot_replace_them()
 {
    let mut h = Harness::new();
    h.until(|v| v.page.is_some() && !v.busy && !v.comparison.is_loading());
    let hashes = h
        .view
        .page
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .map(|e| e.commit.hash.clone())
        .collect::<Vec<_>>();
    let original = h.view.git.clone();
    let mut missing = (*original).clone();
    missing.executable = h.temp.path().join("missing-git.exe");
    h.view.git = Arc::new(missing);
    h.click(&text("RefreshRepository"));
    h.until(|v| !v.busy);
    assert_eq!(h.view.status, text("ActionReadFailed"));
    assert_eq!(
        h.view
            .page
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .map(|e| e.commit.hash.clone())
            .collect::<Vec<_>>(),
        hashes
    );
    h.view.git = original;
    let obsolete = h.view.generation;
    h.view.cancel_reads();
    let late = || Event {
        generation: obsolete,
        mode: Mode::History,
        result: Ok(ResultData::History(Page {
            entries: vec![],
            next: None,
            offset: 0,
        })),
    };
    h.view.sender.send(late()).unwrap();
    h.view.poll(&h.ctx);
    assert_eq!(h.view.page.as_ref().unwrap().entries.len(), 2);
    assert_eq!(h.view.status, text("ActionResultCancelled"));
    h.view.close();
    h.view.sender.send(late()).unwrap();
    h.view.poll(&h.ctx);
    assert!(!h.view.open);
    assert_eq!(h.view.page.as_ref().unwrap().entries.len(), 2);
}

#[test]
fn file_history_older_newer_buttons_page_real_commits_without_advancing_on_failure() {
    let mut h = Harness::new();
    let root = h.view.context.repository.worktree.clone();
    let mut stream = String::new();
    for i in 1..=203 {
        let message = format!("revision {i}\n");
        let content = format!("line {i}\n");
        stream.push_str(&format!("commit refs/heads/main\nmark :{i}\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{message}",1700000000+i,message.len()));
        stream.push_str(&format!(
            "from {}\n",
            if i == 1 {
                h.view.context.repository.head.clone().unwrap()
            } else {
                format!(":{}", i - 1)
            }
        ));
        stream.push_str(&format!(
            "M 100644 inline tracked.txt\ndata {}\n{content}\n",
            content.len()
        ));
    }
    stream.push_str("done\n");
    let command = |args: &[&str], input: &[u8]| {
        let mut request = h.view.git.request(
            &root,
            args.iter().copied(),
            Lifetime::AcceptedCommand,
            65536,
        );
        request.input = input.to_vec();
        let r = process::run(&request, &Cancellation::default(), None).unwrap();
        assert!(r.success(), "{r:?}");
    };
    command(&["fast-import", "--quiet"], stream.as_bytes());
    command(&["reset", "--hard", "HEAD"], &[]);
    fs::write(root.join("tracked.txt"), "working\n").unwrap();
    let context = Arc::new(
        h.view
            .git
            .working_copy(&root, &Cancellation::default())
            .unwrap(),
    );
    let entry = context
        .entries
        .iter()
        .find(|e| e.path == "tracked.txt")
        .unwrap()
        .clone();
    h.view = Inspector::new(context, entry, h.view.git.clone(), false);
    h.until(|v| v.page.is_some() && !v.busy);
    assert_eq!(h.view.page.as_ref().unwrap().entries.len(), 200);
    let first = h.view.page.as_ref().unwrap().entries[0].commit.hash.clone();
    h.click(&text("FileHistoryOlder"));
    h.until(|v| !v.busy && v.offset() == 200);
    assert_eq!(h.view.page.as_ref().unwrap().entries.len(), 5);
    assert!(h.view.page.as_ref().unwrap().next.is_none());
    h.click(&text("FileHistoryNewer"));
    h.until(|v| !v.busy && v.offset() == 0);
    assert_eq!(h.view.page.as_ref().unwrap().entries[0].commit.hash, first);
    let mut missing = (*h.view.git).clone();
    missing.executable = h.temp.path().join("missing-git.exe");
    h.view.git = Arc::new(missing);
    h.click(&text("FileHistoryOlder"));
    h.until(|v| !v.busy);
    assert_eq!(h.view.offset(), 0);
    assert_eq!(h.view.page.as_ref().unwrap().entries.len(), 200);
    assert_eq!(h.view.status, text("ActionReadFailed"));
}
