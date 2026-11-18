use super::*;
use crate::{
    strings::text,
    workspace_interaction_tests::{frame, sidebar_click, workspace},
};
use eframe::egui::Color32;
use shovel_core::settings::SettingsStore;
use std::{sync::mpsc, thread};

fn commit(email: &str) -> Commit {
    let date = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap();
    Commit {
        hash: "b".repeat(40),
        parents: vec![],
        author: "Fixture".into(),
        email: email.into(),
        authored: date,
        committer: "Fixture".into(),
        committer_email: email.into(),
        committed: date,
        message: "subject".into(),
    }
}
fn until(done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(3));
    }
}

#[test]
fn scrolling_past_the_last_visible_author_cancels_its_read_and_drops_the_late_picture() {
    let ctx = egui::Context::default();
    let mut app = workspace();
    install(&ctx, app.read_workers.clone());
    let service = session(&ctx).unwrap();
    AvatarPreferences {
        provider: AvatarProvider::Gravatar,
        site: String::new(),
    }
    .save(&mut app.settings);
    app.refresh_avatar_sources(&ctx);
    let (sent, received) = mpsc::channel();
    let path = app.tabs[0].path.clone();
    let author = commit("visible@example.test");
    ctx.run_ui(Default::default(), |ui| {
        let sent = sent.clone();
        service.picture(ui.ctx(), &path, &author, move |_, cancel| {
            sent.send(()).unwrap();
            until(|| cancel.is_cancelled());
            Some(ColorImage::filled([4, 4], Color32::RED))
        });
    })
    .drop_without_applying_deltas();
    received.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(!app.read_workers.idle());
    ctx.run_ui(Default::default(), |_| {})
        .drop_without_applying_deltas();
    until(|| app.read_workers.idle());
    assert!(service.state.lock().unwrap().cache.is_empty());
}

#[test]
fn repository_pictures_prefer_origin_and_expire_both_images_and_misses_at_the_cache_limit() {
    use shovel_core::remotes::{Configuration, Remote};
    let ctx = egui::Context::default();
    let mut app = workspace();
    install(&ctx, app.read_workers.clone());
    let session = session(&ctx).unwrap();
    let root = app.tabs[0].path.clone();
    let remote = |name: &str, location: &str| Remote {
        name: name.into(),
        location: location.into(),
        push_location: String::new(),
        can_edit: true,
    };
    app.tabs[0].remotes.value = Some(Configuration {
        stamp: "fixture".into(),
        remotes: vec![
            remote("another", "https://github.com/wrong/repository"),
            remote("origin", "git@github.com:team/correct.git"),
        ],
    });
    app.refresh_avatar_sources(&ctx);
    let source = Source::GitHub("team/correct".into());
    let oldest = Key {
        source: source.clone(),
        author: "old@example.test".into(),
    };
    let now = Instant::now();
    {
        let mut state = session.state.lock().unwrap();
        assert_eq!(state.sources.get(&root), Some(&source));
        state.cache.insert(
            oldest.clone(),
            Cached {
                created: now - Duration::from_secs(3601),
                image: None,
            },
        );
        for i in 1..128 {
            state.cache.insert(
                Key {
                    source: source.clone(),
                    author: format!("{i}@example.test"),
                },
                Cached {
                    created: now,
                    image: None,
                },
            );
        }
    }
    session.picture(&ctx, &root, &commit("new@example.test"), |_, _| None);
    until(|| app.read_workers.idle());
    let new = Key {
        source: source.clone(),
        author: "new@example.test".into(),
    };
    {
        let mut state = session.state.lock().unwrap();
        assert_eq!(state.cache.len(), 128);
        assert!(!state.cache.contains_key(&oldest));
        state.cache.get_mut(&new).unwrap().created = now - Duration::from_secs(3601);
    }
    let (sent, recv) = mpsc::channel();
    session.picture(
        &ctx,
        &root,
        &commit("NEW@example.test"),
        move |request, _| {
            sent.send(request.source.clone()).unwrap();
            Some(ColorImage::filled([2, 2], Color32::BLUE))
        },
    );
    assert_eq!(recv.recv_timeout(Duration::from_secs(3)).unwrap(), source);
    until(|| app.read_workers.idle());
    assert!(
        session
            .state
            .lock()
            .unwrap()
            .cache
            .get(&new)
            .unwrap()
            .image
            .is_some()
    );
    app.tabs[0].view.identity_blocked = true;
    app.refresh_avatar_sources(&ctx);
    assert!(
        session
            .picture(&ctx, &root, &commit("new@example.test"), |_, _| panic!(
                "replaced owner"
            ))
            .is_none()
    );
}
#[test]
fn visible_authors_deduplicate_and_settings_changes_discard_inflight_images() {
    let mut app = workspace();
    let ctx = egui::Context::default();
    install(&ctx, app.read_workers.clone());
    let session = session(&ctx).unwrap();
    let path = app.tabs[0].path.clone();
    let c = commit(" person@example.test ");
    let never = |_: &Request, _: &Cancellation| -> Option<ColorImage> {
        panic!("unexpected image request")
    };
    assert!(session.picture(&ctx, &path, &c, never).is_none());
    let mut prefs = AvatarPreferences {
        provider: AvatarProvider::Gravatar,
        site: String::new(),
    };
    prefs.save(&mut app.settings);
    app.refresh_avatar_sources(&ctx);
    let (started, rx) = mpsc::channel();
    let mut releases = Vec::new();
    for author in [&c, &commit("second@example.test")] {
        let (send, wait) = mpsc::channel();
        releases.push(send);
        let started = started.clone();
        assert!(
            session
                .picture(&ctx, &path, author, move |_, cancel| {
                    started.send(()).unwrap();
                    wait.recv_timeout(Duration::from_secs(5)).unwrap();
                    assert!(cancel.is_cancelled());
                    Some(ColorImage::filled([4, 4], Color32::RED))
                })
                .is_none()
        );
        rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(session.picture(&ctx, &path, author, never).is_none());
    }
    assert!(
        session
            .picture(&ctx, &path, &commit("third@example.test"), never)
            .is_none()
    );
    assert_eq!(session.state.lock().unwrap().active.len(), 2);
    prefs.provider = AvatarProvider::Initials;
    prefs.save(&mut app.settings);
    app.refresh_avatar_sources(&ctx);
    for send in releases {
        send.send(()).unwrap();
    }
    until(|| app.read_workers.idle());
    assert!(session.state.lock().unwrap().cache.is_empty());
    assert!(session.picture(&ctx, &path, &c, never).is_none());

    prefs.provider = AvatarProvider::Gravatar;
    prefs.save(&mut app.settings);
    app.refresh_avatar_sources(&ctx);
    session.picture(&ctx, &path, &c, |_, _| {
        Some(ColorImage::filled([8, 4], Color32::GREEN))
    });
    until(|| app.read_workers.idle());
    let photo = session
        .picture(&ctx, &path, &commit("PERSON@EXAMPLE.TEST"), never)
        .unwrap();
    let out = ctx.run_ui(Default::default(), |ui| {
        let (area, _) = ui.allocate_exact_size(egui::vec2(44., 44.), egui::Sense::hover());
        crate::history_row::paint_avatar(
            ui,
            area,
            &c,
            &crate::workspace_style::Palette::current(ui),
            &path,
        );
    });
    let meshes = ctx.tessellate(out.shapes.clone(), out.pixels_per_point);
    let mesh = meshes
        .iter()
        .find_map(|p| match &p.primitive {
            egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == photo.id() => Some(mesh),
            _ => None,
        })
        .expect("history paints the cached author image");
    assert!(
        mesh.vertices
            .iter()
            .all(|v| (0.24..=0.76).contains(&v.uv.x))
    );
    out.drop_without_applying_deltas();
    assert!(
        session
            .picture(&ctx, &path, &commit("missing@example.test"), |_, _| None)
            .is_none()
    );
    until(|| app.read_workers.idle());
    assert!(
        session
            .picture(&ctx, &path, &commit("missing@example.test"), never)
            .is_none()
    );
}

#[test]
fn history_settings_apply_author_pictures_only_after_a_successful_save() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = SettingsStore::new(temp.path().into());
    let mut app = workspace();
    app.tabs.clear();
    app.settings = store.load().unwrap();
    app.store = Some(store);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    install(&ctx, app.read_workers.clone());
    app.settings_dialog = Some(crate::settings_ui::Dialog::new(&app.settings));
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
    }
    for key in ["SettingsPageHistory", "SettingsAvatarGravatar"] {
        sidebar_click(&ctx, &mut app, &text(key), egui::PointerButton::Primary);
    }
    assert_eq!(
        session(&ctx).unwrap().state.lock().unwrap().prefs.provider,
        AvatarProvider::Repository
    );
    sidebar_click(
        &ctx,
        &mut app,
        &text("SettingsSave"),
        egui::PointerButton::Primary,
    );
    let start = Instant::now();
    while app.settings_dialog.is_some() {
        app.receive(&ctx);
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(3));
    }
    assert_eq!(
        session(&ctx).unwrap().state.lock().unwrap().prefs.provider,
        AvatarProvider::Gravatar
    );
    let saved = SettingsStore::new(temp.path().into()).load().unwrap();
    assert_eq!(
        AvatarPreferences::from_settings(&saved).provider,
        AvatarProvider::Gravatar
    );
    // A competing version of the profile rejects the write and preserves the user's pending choice.
    let mut competing = SettingsStore::new(temp.path().into());
    let mut changed = competing.load().unwrap();
    changed.set("FixtureWriter", serde_json::json!("other"));
    competing.save(&changed).unwrap();
    app.settings_dialog = Some(crate::settings_ui::Dialog::new(&app.settings));
    for key in [
        "SettingsPageHistory",
        "SettingsAvatarInitials",
        "SettingsSave",
    ] {
        sidebar_click(&ctx, &mut app, &text(key), egui::PointerButton::Primary);
    }
    let start = Instant::now();
    while app.saving {
        app.receive(&ctx);
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(3));
    }
    let dialog = app.settings_dialog.as_ref().unwrap();
    assert!(dialog.error.is_some());
    assert_eq!(
        AvatarPreferences::from_settings(&dialog.values).provider,
        AvatarProvider::Initials
    );
    assert_eq!(
        session(&ctx).unwrap().state.lock().unwrap().prefs.provider,
        AvatarProvider::Gravatar
    );
    sidebar_click(
        &ctx,
        &mut app,
        &text("Cancel"),
        egui::PointerButton::Primary,
    );
    assert!(app.settings_dialog.is_none());
    let actual = SettingsStore::new(temp.path().into()).load().unwrap();
    assert_eq!(actual.get("FixtureWriter").unwrap(), "other");
    assert_eq!(
        AvatarPreferences::from_settings(&actual).provider,
        AvatarProvider::Gravatar
    );
}
