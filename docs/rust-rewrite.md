# Independent Rust application

The owner requested the entire Shovel desktop application in Rust on 2026-11-05. Baseline C# source is `2175458` (0.47.0). This branch is in progress and has no delivered Rust application yet. [TODO](../TODO.md#entire-rust-application--active-not-delivered) owns the complete parity checklist; historical C# milestones are not Rust evidence.

The Rust workspace uses a pinned MSVC toolchain with core, desktop and askpass crates. The desktop uses egui/eframe 0.37.0 with native Windows/WGPU rendering, AccessKit and system fonts. It does not use a browser or start the C# application. The internal workspace currently wires repository opening/restoration, asynchronous history paging, graph/metadata/file comparisons, working-copy staging/hunks/Commit/drafts, explicit Fetch/Pull/Push and profile saving; it is not the complete application. Git for Windows remains the configured Git engine so hooks, filters, credentials and tools retain their native behavior. Existing C# source/assets/resources serve as the parity reference and are excluded from the Rust runtime package.

## Network and authentication ownership

Cached split-button options do no Git or filesystem work. Changed flags are saved asynchronously even when the menu is dismissed; explicit destinations retain the reviewed branch, while implicit destinations resolve current Git configuration when executing. Accepted requests belong to the application scheduler, with Fetch separate from working-copy mutations and Pull/Push exclusive across the shared repository. The captured identity/branch/HEAD guard remains; a queued request whose HEAD changes before its worker starts currently fails with a stale-state explanation. This needs further parity review for queues following another accepted commit.

The native network layer validates configuration and refs, uses shared C#/Rust named mutation locks, suppresses implicit pruning/recursion/force/mirror behavior and retains partial results. Pull also owns the Fetch lock through immutable FETCH_HEAD/tracking-tip verification and merge/rebase; ignored-path collisions and zero-exit autostash conflicts remain visible. Captured output is bounded and URL-redacted before presentation; arbitrary raw pipe chunks are not exposed as a credential-redaction boundary. Read failures after a successful native command remain separate from that command's actual outcome.

`ShovelRustAskPass.exe` uses a per-operation current-user Windows pipe, random nonce and server-PID verification. Overlapped I/O drains before buffers/handles are released, with bounded prompts/replies and a two-minute prompt deadline. The implementation follows Microsoft's [pipe access-control](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights) and [overlapped-I/O lifetime](https://learn.microsoft.com/en-us/windows/win32/ipc/synchronous-and-overlapped-input-and-output) documentation, checked 2026-11-05. The Rust UI masks replies, clears editor undo state, supplies SSH confirmation/information responses, and closes prompts when their broker ends. Declining or timing out supplies no answer; it never kills an accepted Git command. No credential/prompt persistence is added. Actual GCM/SSH accounts, security-key interactions, native modal ownership/focus and the complete authentication appearance remain unverified.

## Independent channels

`distribution/rust-channels.json` defines `ShovelRust`, `ShovelRust.exe`, the independent `ShovelRust/Foundation` profile, `codex/rust-rewrite` development and `codex/rust-beta` / `codex/rust-stable` release branches. All three branches exist locally; the release branches initially point to foundation commit `01c7593`. Beta is the initial channel. Importing C# preferences copies into the Rust profile and translates the update preference; it never writes the C# profile or shares updater trust files. Draft copying and complete nested preference normalization remain pending.

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
