# Independent Rust application

The owner requested the entire Shovel desktop application in Rust on 2026-11-05. Baseline C# source is `2175458` (0.47.0). This branch is in progress and has no delivered Rust application yet. [TODO](../TODO.md#entire-rust-application--active-not-delivered) owns the complete parity checklist; historical C# milestones are not Rust evidence.

The Rust workspace uses a pinned MSVC toolchain and separate core/UI/helper crates. The desktop must render through a native Rust UI toolkit with Windows integration; it must not wrap or start the C# application. Git for Windows remains the configured Git engine so hooks, filters, credentials and tools retain their native behavior. Existing C# source/assets/resources serve as the parity reference and are excluded from the Rust runtime package.

## Independent channels

`distribution/rust-channels.json` defines `ShovelRust`, `ShovelRust.exe`, the independent `ShovelRust/Foundation` profile, `codex/rust-rewrite` development and `codex/rust-beta` / `codex/rust-stable` release branches. Beta is the initial channel. Importing C# preferences must copy into the Rust profile and translate the update preference; it must never write the C# profile or share updater trust files.

| Identity | Rust Beta | Rust Stable |
| --- | --- | --- |
| Feed tag | `rust-beta` | `rust-stable` |
| Channel | `win-rust-beta` | `win-rust-stable` |
| Feed file | `releases.win-rust-beta.json` | `releases.win-rust-stable.json` |
| Release branch | `codex/rust-beta` | `codex/rust-stable` |
| Release tag | `rust-vVERSION-win-rust-beta` | `rust-vVERSION-win-rust-stable` |
| Full package | `ShovelRust-VERSION-win-rust-beta-full.nupkg` | `ShovelRust-VERSION-win-rust-stable-full.nupkg` |
| Publication | Authorized | Explicit owner approval required |

Both Rust channels use dedicated feed tags, never GitHub `latest` or C# `beta`. Both are GitHub prereleases with `make_latest=false`, including future Rust Stable: the user-facing stable channel is defined by its signed feed, while GitHub latest remains exclusively the C# client's Stable endpoint. Stable's feed location is reserved, with no public assets until approved. Existing public-key identity signs a distinct Rust envelope containing product, channel, architecture, source commit and the Velopack asset; consumers verify signature and all identities before deriving a download URL. Reusing the publisher key does not authorize cross-product packages.

Release packages and profiles use `ShovelRust` to coexist with `Shovel`. Beta/Stable share only the Rust installation identity, with Rust-only channel changes and no downgrade. Versions come from workspace Cargo metadata; C# `Directory.Build.props` remains unchanged. Native installer/updater integration uses Velopack's Rust SDK, not its C# application host. The upstream [Rust integration](https://docs.velopack.io/getting-started/rust) was checked 2026-11-05; packaging/tool compatibility must be verified against the pinned actual crate.

## Verification boundaries

Do not publish an incomplete shell as the requested rewrite. Build checks, pure parsers, a downloaded package and an installed update are different evidence. The requested native update rehearsal uses the isolated Rust installation only; no installation over or restart of C# is authorized. Every unfinished parity row remains visible in TODO. Same-host paired performance measurements must report fixture, build, process-tree scope, raw samples and variation; no faster/smaller claim precedes measurement.
