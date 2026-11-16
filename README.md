# Shovel — native Rust desktop Git client

Master now contains the Rust application, including its desktop UI and askpass helper. The former C# implementation is retained in Git history at [codex/csharp-archive-0.47.0](https://github.com/GlebHmarsky/Shovel/tree/codex/csharp-archive-0.47.0). Rust parity is still in progress: see [TODO](TODO.md) and [evidence](docs/rust-rewrite.md).

## Windows builds

[GitHub Actions](https://github.com/GlebHmarsky/Shovel/actions/workflows/rust-windows.yml) builds Windows x64 on each master push and manual run. It runs release-policy checks, Clippy and bounded smoke, produces an optimized native payload and signed-feed installer, uploads build artifacts, and publishes each new Cargo version as a Rust Beta prerelease. Existing published versions are never overwritten. [Rust releases](https://github.com/GlebHmarsky/Shovel/releases) use rust-vVERSION-win-rust-beta tags. The rust-beta feed serves only ShovelRust; old C# feeds remain unchanged. Rust Stable requires separate owner approval.

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

Keep ShovelRustAskPass.exe next to ShovelRust.exe. Launch with --data-directory PATH for an isolated test profile. Native UI acceptance, final-version installed updates and paired C#/Rust benchmarks remain distinct from build/smoke checks. Historical product/architecture/roadmap sections document the parity baseline; old C# build commands and source paths are not current master commands. Old paired C# fixture scripts are preserved in the Rust development history at bb278e1 for future baseline extraction.
