# Project guidance

## Rust on master — owner approved 2026-11-16

Master is the native Rust Shovel application and askpass helper. The owner explicitly requested replacing the C# tree and starting GitHub Windows builds. This supersedes all older no-merge/C#-master instructions. C# history is preserved at codex/csharp-archive-0.47.0 (2175458). Do not restore C# application projects into master.

Read docs/product.md, docs/requirements.md, docs/architecture.md, docs/roadmap.md, then TODO.md; README.md owns current commands. Read [the Shovel UI style guide](docs/style-guide.md) before changing desktop windows, dialogs or controls. Historical C# documentation is baseline evidence, not the current stack. TODO remains the single parity checklist; merging does not complete unchecked functionality.

- Rust/Cargo owns versions. Native Rust UI and askpass; no C# host or WebView wrapper. Resources live under rust/assets and rust/desktop/resources.
- Keep Git/process/filesystem work off the UI thread. Menus use cached state. Deliver working behavior/progress/errors, never placeholders.
- Accepted user commands must finish even after tab/view closure; refuse app exit while accepted work remains. Cancel waiting requests or automatic fetch only. Preserve failed drafts/index and native Git protections, hooks, filters and authentication.
- No repository-trust dialogs or extra approval layers for ordinary configured Git programs. Confirm genuinely destructive actions with actual targets.
- Preserve unrelated work. No parallel agents unless authorized. Use disposable repositories and isolated Git configuration in tests. No desktop launch, screen capture or installation without authorization.
- Run relevant focused checks, all-target Clippy, git diff --check, then the bounded 18-check smoke suite once on final source. Smoke maximum 30 checks / 180 seconds overall / 30 seconds per check. Do not conceal slow checks by raising deadlines.
- Commit intended source/docs before packaging. Increment Cargo version and CHANGELOG per delivery; never overwrite published versions. Concrete commit titles describe behavior.
- GitHub Actions builds Windows on master and publishes new versions to Rust Beta using the existing update signing key stored as RUST_UPDATE_PRIVATE_KEY. No key replacement or plaintext key in Git/artifacts/logs. Build-Release.ps1 is Rust-only, default win-rust-beta. Stable normally requires explicit owner approval and completed parity. On 2026-11-17 the owner explicitly approved promoting existing 0.50.3 despite unfinished parity; distribution/rust-stable-0.50.3-approval.json records this one-version exception. It is not blanket approval for later Stable releases.
- Preserve ShovelRust package/profile identity and win-rust-beta / win-rust-stable feeds. Existing C# Shovel releases, beta/win-beta/win-stable feeds and GitHub Latest remain intact. Feed signing is separate from Authenticode signing.
- Record exact source, checks and verification limits. Builds do not prove UI acceptance, installed updates or performance. Paired benchmark and final installed-update parity requirements remain open.
