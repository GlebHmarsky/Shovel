
pub fn acceptance_probe() -> bool {
    let args = std::env::args().collect::<Vec<_>>();
    let Some(mode) = args.get(1).filter(|s| s.starts_with("--lifecycle-")) else { return false; };
    let root = PathBuf::from(std::env::var_os("SHOVEL_LIFECYCLE_ROOT").expect("fixture root"));
    let result: Result<(), String> = (|| {
        let cancel = Cancellation::default();
        let git = shovel_core::git::Git::discover(None, &cancel).map_err(failure)?;
        let repo = git.inspect(&root.join("repo"), &cancel).map_err(failure)?;
        let drafts = shovel_core::drafts::Store { directory:root.join("Drafts"), legacy_directory:None };
        let expected = shovel_core::drafts::Text {subject:"Retained across native update".into(), description:"Unicode draft \u{2713}".into()};
        let mut settings = shovel_core::settings::SettingsStore::new(root.join("Profile"));
        if mode == "--lifecycle-initialize" {
            let (mut s,_) = settings.load_or_import(&root.join("no-legacy-profile")).map_err(failure)?;
            s.set("UiScale",serde_json::json!(1.3));
            s.set("AutoFetchEnabled",serde_json::json!(false));
            s.set("RepositoryPaths",serde_json::json!([root.join("repo")]));
            settings.save(&s).map_err(failure)?;
            drafts.save(&repo.identity,None,&expected,&cancel).map_err(failure)?;
        } else if mode == "--lifecycle-update" {
            other_instances()?;
            let offer = check(Channel::Beta,&cancel)?.ok_or("No newer fixture update")?;
            let manager = manager(&offer,cancel)?;
            manager.download_updates(&UpdateInfo {TargetFullRelease:offer.asset.clone(),..Default::default()},None).map_err(failure)?;
            offer.release.verify_package(File::open(package_path(&offer)?).map_err(failure)?).map_err(failure)?;
            fs::write(root.join("download-verified.json"),serde_json::to_vec(&serde_json::json!({"From":env!("CARGO_PKG_VERSION"),"To":offer.release.version(),"SignatureAndPackageVerified":true})).unwrap()).map_err(failure)?;
            manager.wait_exit_then_apply_updates(&offer.asset,true,true,["--lifecycle-restarted"]).map_err(failure)?;
        } else if mode == "--lifecycle-restarted" {
            let (s,_) = settings.load_or_import(&root.join("no-legacy-profile")).map_err(failure)?;
            if (s.ui_scale()-1.3).abs()>0.001 || drafts.load(&repo.identity,&cancel).map_err(failure)?.text != expected {return Err("Settings or draft changed".into());}
            fs::write(root.join("restarted.json"),serde_json::to_vec(&serde_json::json!({"Version":env!("CARGO_PKG_VERSION"),"SettingsLoaded":true,"DraftLoaded":true,"ProcessId":std::process::id()})).unwrap()).map_err(failure)?;
        } else {return Err("Unknown fixture action".into());}
        Ok(())
    })();
    if let Err(e)=result {
        let _=fs::write(root.join("failure.txt"),e);
        std::process::exit(1);
    }
    true
}
