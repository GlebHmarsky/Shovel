//! Console-only packaging probe using exactly the production trust boundary.
//! Not shipped in the desktop payload and never starts an application/window.
use shovel_core::updates::{Channel, MAX_FEED_BYTES, verify_feed};
use std::{fs::File, io::Read, path::Path};

fn bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err("verification input is too large".into());
    }
    Ok(bytes)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 6 {
        return Err(
            "usage: verify_update_bundle CHANNEL VERSION COMMIT FEED SIGNATURE PACKAGE".into(),
        );
    }
    let channel = match args[0].to_str() {
        Some("win-rust-beta") => Channel::Beta,
        Some("win-rust-stable") => Channel::Stable,
        _ => return Err("only a Rust channel can be verified".into()),
    };
    let feed = bounded(Path::new(&args[3]), MAX_FEED_BYTES)?;
    let signature = bounded(Path::new(&args[4]), 2048)?;
    let release = verify_feed(channel, &feed, &signature)?;
    if Some(release.version().to_string().as_str()) != args[1].to_str()
        || Some(release.source_commit()) != args[2].to_str()
        || Path::new(&args[5]).file_name().and_then(|p| p.to_str()) != Some(release.file_name())
    {
        return Err("verified release does not match the expected build".into());
    }
    release.verify_package(File::open(&args[5])?)?;
    println!(
        "{}",
        serde_json::json!({"Verified":true,"Product":"ShovelRust","Channel":channel.id(),"Version":release.version(),"SourceCommit":release.source_commit(),"Package":release.file_name(),"Size":release.size(),"PackageUrl":release.package_url()})
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
