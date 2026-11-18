# Shovel — native Rust desktop Git client

Master now contains the Rust application, including its desktop UI and askpass helper. The former C# implementation is retained in Git history at [codex/csharp-archive-0.47.0](https://github.com/GlebHmarsky/Shovel/tree/codex/csharp-archive-0.47.0). Rust parity is still in progress: see [TODO](TODO.md) and [evidence](docs/rust-rewrite.md).

## Windows builds

[GitHub Actions](https://github.com/GlebHmarsky/Shovel/actions/workflows/rust-windows.yml) builds Windows x64 on each master push and manual run. It runs release-policy checks, Clippy and bounded smoke, produces an optimized native payload and signed-feed installer, uploads build artifacts, and publishes each new Cargo version as a Rust Beta prerelease. Existing published versions are never overwritten. [Rust releases](https://github.com/GlebHmarsky/Shovel/releases) use rust-vVERSION-win-rust-beta tags. The rust-beta feed serves only ShovelRust; old C# feeds remain unchanged. Rust Stable requires separate owner approval.

To finish an interrupted Beta publication, rerun the Windows workflow on the original tagged commit. For a draft, it restores the original installer archive while GitHub retains it and verifies any existing uploads. For a public version, it downloads the original release, verifies its signature, package and installer, then repairs a missing or stale feed asset. A subsequent master commit with the same Cargo version produces a build artifact.

`scripts/Test-RustPublicBeta.ps1` independently checks a completed Beta run and downloads all seven assets. Pass the version, exact source commit, GitHub run ID and the production verifier built with `cargo build --locked -p shovel-core --example verify_update_bundle`. The script verifies the tag, signed feed, package and installer, compares the live alias and writes a receipt under `Build`.

If the GitHub metadata API is rate-limited, pass an existing GitHub login token as a PowerShell SecureString via `-MetadataToken`. Asset downloads use the public endpoints.

Requirements: Windows MSVC build tools, Git for Windows, pinned Rust from rust-toolchain.toml, PowerShell 7.4+, and .NET 10 SDK for the Velopack packaging tool only. The application contains no .NET runtime or C# host.

```powershell
./scripts/Test-RustReleasePolicy.ps1
./scripts/Test-RustSmoke.ps1
./scripts/Build-RustPayload.ps1 -OutputDirectory "$PWD/Build/local-test"
# Signed installer without publication (clean committed source required):
./scripts/Build-Release.ps1 -PackageOnly
# Publish a new signed Rust Beta:
./scripts/Build-Release.ps1
```

The local signing key remains outside Git at ~/.shovel-signing/shovel-update.key. GitHub uses the encrypted RUST_UPDATE_PRIVATE_KEY repository secret containing that same key; it is materialized only in runner temporary storage and removed after packaging. Publisher/Authenticode signing is not configured.

Keep ShovelAskPass.exe next to Shovel.exe. Launch with --data-directory PATH for an isolated test profile. Native UI acceptance, final-version installed updates and paired C#/Rust benchmarks remain distinct from build/smoke checks. Historical product/architecture/roadmap sections document the parity baseline; old C# build commands and source paths are not current master commands. Old paired C# fixture scripts are preserved in the Rust development history at bb278e1 for future baseline extraction.

## Rust Stable 0.50.3 promotion

The owner explicitly approved promoting current 0.50.3 to Rust Stable despite unfinished parity. This is a one-version exception recorded in distribution/rust-stable-0.50.3-approval.json, not blanket approval. scripts/Promote-RustStable.ps1 verifies the existing signed Beta package, repackages unchanged application files for win-rust-stable, and signs/verifies the new feed. Pass -Publish only for the recorded promotion. The Cargo version and application source stay unchanged. Normal Stable parity gates remain in Build-Release.

The Stable versioned release is not a GitHub prerelease, but never replaces the legacy C# Latest release. The mutable rust-stable metadata alias remains a prerelease. In version 0.50.3, choose Rust Stable in update settings; fresh profiles still default to Rust Beta. No application-default or installed-update behavior change is claimed by promotion.
