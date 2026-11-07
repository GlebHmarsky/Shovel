# Shovel

**Rust rewrite worktree — in progress.** This branch preserves C# 0.47.0 as the parity reference and is not a delivered Rust application yet. [Rust design/channel identities](docs/rust-rewrite.md) and the [active parity checklist](TODO.md#entire-rust-application--active-not-delivered) describe the independent implementation. Existing C# release channels and master remain unchanged.

To build a signed Rust preview installer, commit the Rust source and run `./scripts/Build-Release.ps1 -Channel win-rust-beta -PackageOnly`. It runs the explicit 18-check smoke suite, builds both native executables and packages `Build/rust-0.49.2-installer-win-rust-beta/ShovelRust-Setup.exe` with a signed Rust-only feed. The existing private update key stays outside Git. PackageOnly does not publish a GitHub release or merge branches. The owner authorized partial Rust Beta publication on 2026-11-07: omit PackageOnly to publish the immutable prerelease and signed Rust Beta feed, then verify anonymous downloads. Publication reuses Git Credential Manager without requiring GitHub CLI. TODO remains the full parity acceptance list; Rust Stable requires complete parity and explicit owner approval.

For a local executable alone, run `./scripts/Build-RustPayload.ps1`. Run `app/ShovelRust.exe` with its adjacent askpass helper. `VERIFICATION.json` records source fingerprint, executable hashes and smoke coverage. File → Settings edits preferences. View → Activity log shows output; clicking repository context also opens it. Help → Shovel Rust updates checks the selected independent Rust channel. Downloads and restart are explicit; a channel with no published feed reports that limitation. An actual installed update has not yet been certified.

Rust focused development commands (PowerShell, from this worktree; select the relevant check):

```powershell
. ./scripts/Use-RustToolchain.ps1
cargo test -p shovel-core --test working_copy --locked
cargo test -p shovel-core --test drafts --locked
cargo test -p shovel-core --test hunks --locked
cargo test -p shovel-core --test network --locked
cargo test -p shovel-core --test branches --locked
cargo test -p shovel-core --test history_actions --locked
cargo test -p shovel-core --test stashes --locked
cargo test -p shovel-core --test conflicts --locked
cargo test -p shovel-core --test file_actions --locked
cargo test -p shovel-core --test file_history --locked
cargo test -p shovel-core --test file_actions patch_ --locked
cargo test -p shovel-core patches::tests --lib --locked
cargo test -p shovel-core file_rules::tests --lib --locked
cargo test -p shovel-core conflict_text::tests --lib --locked
cargo test -p shovel-core recovery::tests --lib --locked
cargo test -p shovel-core authentication::tests --lib --locked
cargo test -p shovel-askpass --test git_helper --locked
cargo test -p shovel-core scheduler::tests --lib --locked
cargo build -p shovel-askpass --locked
cargo test -p shovel-desktop operations::tests --locked
cargo test -p shovel-desktop branches::tests --locked
cargo test -p shovel-desktop stashes::tests --locked
cargo test -p shovel-desktop commit::conflict_tests --locked
cargo test -p shovel-desktop file_history::tests --locked
cargo test -p shovel-desktop file_inspection_menu --locked
cargo test -p shovel-desktop commit::file_menu_tests --locked
cargo test -p shovel-desktop commit::file_menu_tests::shell_ --locked -- --test-threads=1
cargo clippy -p shovel-core -p shovel-askpass -p shovel-desktop --all-targets --locked -- -D warnings
cargo build -j 1 -p shovel-desktop -p shovel-askpass --locked
./scripts/Test-RustReleasePolicy.ps1
```

Toolchain: Rust 1.99.1 MSVC, pinned by `rust-toolchain.toml`; Visual Studio C++ tools and Windows SDK. These are focused development checks, not full parity or installer acceptance. The C# commands and release evidence below are the baseline reference.

The development executable accepts `--data-directory <absolute-profile-folder>` for an isolated profile. Rust defaults to `%LOCALAPPDATA%/ShovelRust/Foundation`; C# settings/drafts are copied on demand and never written. Use disposable repositories for mutation verification. The native Commit presentation-state check uses a real held hook and isolated Git configuration without showing a desktop window. Live UI verification remains incomplete; see [dated evidence](docs/roadmap.md#independent-rust-rewrite--active-2026-11-05).

Keep `ShovelRustAskPass.exe` beside `ShovelRust.exe`; explicit network commands require that Rust helper. The focused desktop operations check also requires `cargo build -p shovel-askpass` first. Network fixtures use isolated local bare remotes; credential-fill fixtures invoke the compiled helper without contacting a server. Branch/checkout checks use disposable repositories and headless pointer input; they never delete a user's branch. These checks do not validate live GCM/SSH accounts, desktop ownership/appearance or installed distribution. The branch-management and history/recovery suites are optional focused coverage, not routine smoke. Do not aggregate their scenarios into one default check; the branch-management and selected-LFS discard fixtures exceed 30 seconds.

**0.47.0 Beta:** compact stash topology, checkout choices, remembered network options, persistent window/pane/sidebar state and shared-state refactoring. [Behavior and limits](docs/product.md#checkout-and-remembered-workspace-state-0470-beta). Stable remains **0.39.1**.

Windows 10/11 Git client, C# / WPF / .NET 10 LTS. Choose Stable or Beta in Help → Shovel updates. Launch checks discover updates; opening that window checks immediately, and **Update and restart** approves the download and guarded restart. Every task delivers a signed-feed Beta installer through Build-Release; routine smoke remains 26 checks.

[AGENTS](AGENTS.md) owns session guidance; [product](docs/product.md) UX; [requirements](docs/requirements.md) acceptance; [architecture](docs/architecture.md) mechanisms/limits; [roadmap](docs/roadmap.md) evidence; [TODO](TODO.md) implementation priorities.

**Delivery policy:** requested controls must perform real actions; no new placeholders or UI-only completion. Established work environment: no repo-trust/first-use/program-consent layer for ordinary Git actions, and no extra safety measures without concrete need. Normal action buttons authorize the operation and configured hooks/helpers; required auth/destructive confirmations remain. Unfinished entry points are omitted; disabled working actions explain the missing target/dependency or active operation.

Operations show static phase/outcome text and activity logs. Remove waiting requests individually; started user commands finish, and only automatic fetch can be preempted. Elapsed badges, animated progress and fading capsules are removed. `--check-operation-style` runs four focused outcome/static-status/offscreen layout checks without showing a window.

## Prerequisites

Workspace-state checks: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-workspace-state` (11 focused checks) covers native index cache/staging, checkout/hooks/stash recovery, disk layout reload, remembered network flags and sidebar expansion. `--check-history-layout` adds five topology/paging/offscreen checks. Fixtures are disposable; no windows are shown.

Operation-lifetime checks: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-operation-lifetimes` (11 focused checks): held initialization, close/reopen ownership, background Commit, preserved failed drafts/index, waiting removal, failure queues, ordered staging/Commit, background Stash and protected native-process completion. No desktop launch. [Behavior](docs/product.md#repository-owned-work-0460-beta).

Settings/avatar checks: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-settings-pages` (16 focused checks for persistence/failure, seven bound category layouts, diff resources, isolated native Git identity, provider mapping, bounded image loading and update cancellation). Uses mock HTTP and offscreen rendering, with no desktop launch or live-server authentication.

History/file activity checks: `--check-history-file-actions` (11) and `--check-history-activity` (4), using the same `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- <flag>` command. Disposable fixtures cover native restore/export/filters, LFS rules, history/blame, tool cancellation, retained command output and saved label placement; no desktop launch.

External merge-tool checks: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-merge-tool-settings` (ten focused checks: saved settings, form/chooser binding, real custom/named tool execution, paths, failure, stale edits and cancellation). Uses disposable repositories and a console test helper, never a third-party GUI.

Focused settings checks: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-auto-fetch-settings` (eight checks for persistence, concurrent writes, validation/retry, scheduling, cancellation and offscreen layouts). No desktop launch.

Clone/remote checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-network-workflows` (11 focused checks; disposable Git/filter/hook/auth-cancellation fixtures, real bound actions, partial/stale/locked configuration and offscreen Dark/Light forms). No visible app, live server or installation is exercised. [Evidence and limits](docs/roadmap.md#clone-and-remote-management-0330).

QOL checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-qol` (eight bounded checks for retained views/selection, commit overview, saved scale, layout migration, session activity and offscreen layouts). `--check-conflicts` includes seventeen disposable conflict scenarios and inline Commit staging. These are optional focused suites; normal packaging runs the unchanged smoke suite.

Repository workflow checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-repository-workflows` (15 focused checks, including held-fetch Commit and automatic-fetch preemption: parallel repositories, rejected push, F5, auto-fetch cadence/shutdown, commits, graph, proportional panes and attention renders). Uses disposable local remotes; no desktop control or installed launch.

History/caption checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-history-layout` (four opt-in checks; disposable stashes, topology/paging and offscreen caption/toolbar renders). [Caption and folded-stash behavior](docs/product.md#compact-caption-and-history-0260). Native caption interactions require separate authorized desktop validation.

- Windows x64, SDK **10.0.401** (global.json, patch roll-forward) with Desktop targeting pack for development; Visual Studio optional, no C++ toolchain/workload/third-party NuGet packages.
- Run-only machines need matching .NET 10 **Desktop Runtime**; preview is framework-dependent.
- Git for Windows >=2.47.0; tested here 2.51.1. Standard roots discovered automatically, preferring native ucrt64/mingw64/mingw32 over cmd/git.exe. Repository → Git executable selects a custom installation for the session; no PATH/repository-local discovery.
- Recorded host: SDK 10.0.401/Desktop Runtime+pack 10.0.12/Git 2.51.1, already installed. Historical 2026-10-23 support check listed .NET 10.0.13; refresh servicing before distribution. .NET 10 support recorded through 2028-11-14. [Policy](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core).
- Verify coworker Windows editions/builds; Windows 10 runtime compatibility is not support for every edition. [Windows support](https://learn.microsoft.com/en-us/dotnet/core/install/windows). ARM64/installed distribution unverified.

## Restore, build and check

PowerShell from repository root:

```powershell
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$env:DOTNET_CLI_HOME = Join-Path (Get-Location) 'artifacts/dotnet-home'
$env:DOTNET_ADD_GLOBAL_TOOLS_TO_PATH = 'false'
$env:DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE = 'true'
$env:MSBuildEnableWorkloadResolver = 'false'
dotnet restore GitGui.slnx -m:1
dotnet build GitGui.slnx --no-restore -c Release -m:1 -p:UseSharedCompilation=false -nodeReuse:false
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build
```

Default (or `--smoke`): **26 checks**, enforced maximum **30**, target under **120 seconds**, hard stop **180 seconds overall / 30 seconds per check**. Prints the active check and elapsed time, stops at the first failure. Exit 124 means timeout; exit 1 means failure; invalid mode exits 2. The small workflow reuses a disposable repository; later steps depend on earlier success. It covers representative operations, not the full regression matrix.

Optional extended verification, only when relevant:

```powershell
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --full
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-drafts
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-draft-io
```

`--check-draft-io` runs four isolated-file checks for reopen, temporary/persistent locks, cancellation, competing saves and diagnostic privacy, without Git or visible windows. It is opt-in and does not increase the default count.

All existing focused commands and opt-in benchmarks remain available. Full verification retains its detailed checks and longer deadlines; it never runs implicitly. During development run only relevant checks, then run the default once through Build-Preview on final code; avoid separately repeating an unchanged smoke run. The target excludes restore/build/publish time; timers include smoke fixture setup.
Check every exit code. Env applies to current shell; single-process MSBuild/no shared compiler avoids observed execution restrictions. Checks are a console runner, not dotnet test; nonzero means failure.

Routine fixtures under ignored artifacts/smoke-checks; optional full fixtures under artifacts/foundation-checks (initialization destinations and empty nonrepo fixtures in temp), isolated config/disposable Git repos/local bare remotes/test hooks. Headless WPF uses a plain test Application/shared resources, suppresses production startup and saves PNGs without Show/desktop control. Restricted MSYS named-object creation may need sandbox permission; that does not authorize live remotes/launches. [Evidence](docs/roadmap.md#fast-file-actions-and-binary-cards-2026-10-28) distinguishes integration/headless from desktop.

Focused theme resource/control/offscreen checks (no window shown): `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-theme`. The full suite also runs these checks.

## Run

Interactive launch requires task authorization for AI sessions. Manual commands:

```powershell
dotnet run --project src/GitGui.App -c Release --no-build
# Optional isolated profile:
dotnet run --project src/GitGui.App -c Release --no-build -- --data-directory "C:\Users\hmars\Documents\GitGUI\artifacts\manual-profile"
```

Data-directory must be absolute; default %LOCALAPPDATA%\GitGui\Foundation preserves old GitGui tabs/layout. One instance/profile; independent profiles can coexist. Closing cancels/drains reads then saves; no tray/autostart.

Keep Shovel.AskPass.exe/.dll/.deps.json/.runtimeconfig.json beside Shovel (build/publish copies them). Askpass handles explicit network credentials/passphrases/host confirmation; browser/GCM/signing/interactive hooks/custom SSH still use existing tools. Shovel saves no prompt/credential data; configured helpers may store credentials. Live sign-in/focus/input unverified.

## Using the workspace

**0.37.0 conflict text:** the inline editor supports UTF-8 and BOM-marked UTF-16 LE/BE, displays the result encoding and preserves it on save. `--check-conflicts` runs seventeen focused checks, including exact encoded staged bytes, malformed input, byte limits, retained failed saves and headless layouts. [Behavior and limits](docs/product.md#daily-use-interactions-0310).

**0.30.1 discard:** optional `--check-discard` runs six isolated discard checks (partial staging, literal paths, filters/hooks, locks and stale state); `--check-lfs` runs eleven checks covering hydration, selected LFS verification/fallback, concurrent edits, missing objects and responsive completion of accepted restores. `--measure-discard-lfs` measures one and 64 selected assets; `--measure-discard-lfs-trace` additionally reports allowlisted child timings and adds profiling overhead. These commands use disposable repositories and do not touch your repositories. [Results](docs/roadmap.md#lfs-discard-0301).

**0.30.0:** [conflict editor, operation banner, startup cache, stash history menus and tab dragging](docs/product.md#conflict-resolution-and-faster-reopening-0300). Optional focused checks: `--check-conflicts` (17), `--check-usability` (7), `--check-lfs` (11; requires already installed Git LFS). `--check-startup` runs only the two cache checks. Use the same `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- <flag>` command. All use disposable repositories; no visible app or live server.


**History:** right-click a commit for detached Checkout, Reset → Soft/Mixed/Hard, Copy full SHA and Revert. Reset confirms its actual branch/tips and effects; merge Revert asks which numbered parent is the mainline. Results use existing progress, output and recovery. [Behavior and limits](docs/product.md#history-actions-and-network-split-buttons-0290).

**Fetch / Push:** main button executes the configured default target; arrow opens options without preparation. Push uses the configured push destination, which may differ from the Pull upstream; unsupported/multi-ref defaults require explicit options. Commit always opens its workspace.

Optional focused checks: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-history-actions` (nine checks; disposable mutations, local bare remotes, no visible app). `--check-default-network` runs only its two network-target checks. Default smoke remains 26.

**Pull:** the main button uses remembered strategy/auto-stash choices. The arrow edits those choices and optionally an explicit destination. Unchanged destinations continue following Git’s configured upstream; explicitly changed destinations are retained for the current branch. Fetch/Push use the same preference behavior. [Current semantics](docs/product.md#checkout-and-remembered-workspace-state-0470-beta).

Optional focused checks: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-default-pull` (four local-Git/headless checks and Dark/Light normal/minimum renders; no live server or visible app).

**0.27.0:** resize the sidebar freely within the window; labels trim and it scrolls vertically only. Commit remains beside Push; duplicate sidebar title/Local Changes, the page-control band and main footer are removed. Help retains the version. Read failures retain rows with a Diagnostics link. **Repository → Add remote…** saves a name and URL/local path without network access; use Fetch next, then **Set upstream…** (or Unset upstream) there. The Remotes group also offers Add remote. Existing remote edit/delete is deferred.

Optional focused verification: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-workspace-cleanup` (four bounded checks; disposable Git, controlled read failures/cancellation, topology and offscreen Dark/Light renders). Default remains 26 checks.


**0.24.0 workspace:** repository tabs divide the full strip equally. Same-author rows have a yellow background with unchanged text; selected rows keep normal styling. Top-right **Open in** launches Explorer, Rider or VS Code; **Console** opens Git Bash at this repository (Ctrl+Alt+T, Explorer Ctrl+Alt+O). Rider prefers a single root-level `.uproject`, then `.slnx`/`.sln`/`.csproj`; multiple/no candidates open the folder. Missing apps prompt for their executable, remembered locally. **Appearance → Dark / Light** updates immediately and persists. Windows high contrast overrides both. No Work button or startup/menu-time discovery. [Workflow](docs/product.md#workspace-toolbar-and-appearance-0240), [handoff and schema limits](docs/architecture.md#workspace-presentation-and-tool-handoff-0240).

Optional focused checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-workspace` (four checks; isolated settings/Git, fake launch boundary and offscreen renders, no interactive tool launch).

**0.23.0 sidebar:** single-click a branch/tag/fetched remote branch/stash to select its commit; double-click a local branch (or Enter) to switch. Branch right-click offers switch/delete/upstream actions; ↑/↓ are local upstream counts. Remotes offer explicit Fetch. Stashes offer Apply/Pop/confirmed Drop inline; the existing Repository → Stashes inspector retains Create and staged-state options. Submodules open as another tab and offer Initialize / Update to the recorded commit or Sync URL. No force/recursion/automatic commit. Lists load after first history and keep cached rows on failure. [Workflow and limits](docs/product.md#repository-sidebar-0230).

Optional focused sidebar integration: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-sidebar` (four checks). `--render-sidebar` runs only its first list/input-binding/offscreen check. Neither is part of routine verification; no interactive window is shown. History currently supports at most 256 saved stashes, with an explicit error above that bound. Ref jumps have bounded output/time and can fail on very large histories without clearing useful rows.

**0.22.0:** Branch menu adds confirmed local deletion and upstream set/unset. Commit drafts persist locally per worktree; [storage/recovery limits](docs/architecture.md#integrated-branch-management-and-drafts-0220).

Focused integration checks after building: `--check-branches`, `--check-drafts`, `--check-history-performance` passed to the foundation-check runner. Full verification includes these small checks. Opt-in benchmarks: `powershell -NoProfile -File scripts/Measure-History.ps1 -Profile merge-heavy -Samples 3`; `large` and `million` profiles and `-IdleFiveMinutes` are explicit additional runs, not release prerequisites or desktop measurements. See [metric definitions](docs/handoffs/task-3.md).

**File / Repository → New repository…** creates a local repository in a new or existing folder. Enter an absolute path and optional initial branch (blank uses Git's default), then Create. Options open without preparation; execution/progress/output stays in the shell and the resulting repository opens automatically. Existing files remain unstaged; no commit or remote is added. An existing root opens without reinitializing. Partial failure/cancellation preserves files; inspect the result before retrying. [Destination restrictions and workflow](docs/product.md#new-local-repository-0210).

Focused initialization checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-init`. Included in full verification. These use isolated configuration and disposable temp repositories outside the source checkout (nested init is unsupported), with their location recorded in the artifact folder.

Open a local worktree root; selected saved tab restores automatically, others load on selection, no read consent. Fixed All Commits includes local/remote/tag/current-HEAD ancestry, detached included; saved stashes included; other reflog-only/unreachable history excluded, no scope selector. Legacy narrow scope ignored.

Headerless graph/refs/description/author/date/hash begins below tabs. Select row for full parsed metadata/message/parents/refs. View → Older/Newer, Ctrl+Page Up/Down or scrolling past a page edge: 200-row pages/no total count cap. View shows range/status; Diagnostics exposes limits. Three cached tabs/character budget, 128 back cursors/First history page; page/selection/scroll cached in-session; the selected verified page is also saved as a labelled startup preview on clean shutdown. Cache eviction reloads first page.

Select a commit, then **Changes** in the bottom panel for its committed files and numbered text diffs or binary byte sizes. Merge commits default to the first parent; choose another parent to compare that side. Initial commits compare with the empty tree; missing history objects report an error without fetching. Shared file markers are yellow **M**, green **+** and red **−**, with textual/accessibility descriptions. The panel reads committed objects, not your working files.

Cached switch publishes immediately/validates identity off-thread. >30 s snapshots refresh on tab/window activation; Refresh forces read. No history preload/local-status polling; auto-fetch is described in the current behavior above. Refresh retains rows on failure/cancel, changed refs restart page one/retain selected hash if present; changed boundaries block paging. Identity errors clear rows. Right-click tab: refresh/move/close/close others; MMB closes clicked tab, active read cancels/neighbour selected, inactive close preserves active work.

Pull badge = behind, Push = ahead of configured upstream **locally**, loaded after rows; separate push target not compared. Zero measured, dash pending/unavailable/failed; tooltip reason/time/upstream. Missing/detached/unborn/shallow cannot claim synchronized; failed refresh keeps stale counts, no implicit fetch.

Menus, toolbar actions and activity log remain; the top-right tools and sidebar perform real actions. Local changes and the detailed Stashes inspector open owned modals; All Commits stays in the main workspace. Repository holds refresh/cancel/Git selection/Diagnostics/tab actions. Diagnostics includes bounded in-memory stderr/timings, not persistent content logs. Exact-author peers use yellow row backgrounds, selected row normal/high-contrast system colors; [product](docs/product.md) owns exact matching.

## Local Changes

Choose toolbar **Commit (count)**. The resizable modal binds cached rows/draft before its background scan, while history stays behind it. Select unstaged or staged files for the appropriate diff. Ctrl/Shift selects multiple files; hold Shift while clicking the existing Stage/Unstage buttons for all listed files. Enter subject/body to Commit; Amend can preload the last message and confirms the actual tip. Hold Shift on Commit for Commit and Push to the configured upstream. Ctrl+Enter commits from anywhere in this window; Ctrl+Shift+Enter commits and pushes. Successful commit closes the editor; commit failure keeps the message. Push failure retains the successful commit and opens the attention log dialog. Drafts persist locally across modal/tab closure and restart, keyed by worktree identity. Unsaved edits inside the debounce window can still be lost on a crash; storage failures expose recovery information.

The right side shows colored, numbered unified text, whitespace/whole-file options and hunk navigation. Binary files show Old/New cards, extension and size with exact bytes; recognized LFS pointers show asset size without downloading content. Refresh/Cancel and retained Git output are inside the modal. Failed status reads keep rows but disable stale writes. Commit count runs once after selected-tab history, never delays those rows or polls/preloads other tabs. [Product](docs/product.md#local-changes--commit-0130) owns the full workflow; [architecture](docs/architecture.md#local-changes-and-commit-0130) owns limits/filter and submodule restrictions. Select a conflicted file and choose **Resolve conflict…** for named sides, text editing and real per-file staging. Image previews and arbitrary-line staging remain deferred.

Select a line inside a change block, or Ctrl/Shift-select multiple blocks, then use **Stage hunk(s)** / **Unstage hunk(s)** beside the diff. This stages complete blocks, not individual lines. Ordinary tracked text modifications are supported; unavailable file types/options explain their whole-file alternative. [Hunk implementation and limits](docs/architecture.md#selected-hunks-0140).

File Stage/Unstage publishes freshly checked target rows before a coalesced full scan; other rows can remain stale until that background scan. The next action does not wait for an obsolete diff/scan. Selection follows the same file across lists. Bottom activity bars and the ticking footer are removed; static results/errors and cancellation remain. Expand **Git output** for native command output. Internal validation/command/refresh timings remain available to the optional benchmark. The selected diff loads separately; timings remain in memory without diagnostic logging of paths/arguments/content.

Focused headless checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-local` for the parser/status, or `--check-commit` for modal/diff/mutation workflows. Use `--check-hunks` for hunks, `--check-commit-files` for historical files/diffs and `--check-fast-changes` for scoped/background action results, delayed reads, LFS sizes and binary cards. Optional `--full` includes all of them; routine previews use only smoke checks.

Local Changes file menus: right-click a row, or use Shift+F10/Menu key. Stage/Unstage and explicit All entries keep their captured scope regardless of Shift. Open/Explorer accepts one file; Copy path/full path supports multiselection. Enter in the file grid stages/unstages selected files; Delete opens Discard confirmation; Ctrl+C / Ctrl+Shift+C copies relative/full paths; Ctrl+Shift+Alt+S acts on all files on that side; Ctrl+Shift+Alt+O opens one working file. Menu Enter activates its highlighted item. Deleted files use Explorer's containing-folder fallback. Refresh after editing externally. [Workflow/limits](docs/product.md#local-changes-file-menus-0190).

Additional menu commands: confirmed Discard and selected-file Stash; configured External Diff; pinned first-parent file History and committed Blame; Ignore paths/extensions; exact-path LFS tracking/reset rules; binary-capable Save as Patch. Discard in Staged removes **both staged and unstaged** edits at selected paths; Unstaged preserves staged content. Untracked deletion is permanent. [Detailed scopes and recovery](docs/product.md#selected-file-actions-0200). Focused checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-file-menus` or `--check-file-actions`; both are included in full verification. Tests use disposable repositories and a synthetic external diff script, never actual editors/Explorer or the real clipboard.

## Stashes

The sidebar lists stashes directly; choose **Repository → Stashes** for Create and detailed inspection. Cached entries/message appear before background reads. Create accepts an optional message and **Include untracked files**; ignored files and submodule contents stay excluded. Select a stash to inspect saved tracked, staged, unstaged or untracked files with real text/binary diffs. **Apply** keeps the stash; **Pop** removes it only after clean application and an unchanged list. **Restore staged state** is optional for both. **Drop** confirms the exact saved entry and its consequences.

Results/output remain in the modal, with static status and Cancel available. Failed/conflicted application keeps the stash; a partially successful Pop reports applied changes separately from removal. Resolve and stage per file in Local Changes; stash conflicts do not create a Continue/Abort sequence. [Workflow](docs/product.md#stashes-0180), [bounds and concurrent-client limits](docs/architecture.md#stashes-0180).

Focused checks after building: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-stashes`. Disposable repositories and offscreen renders only; available in optional full verification.

## Branch and remote actions

Branch: create from concrete starting commit/optional checkout; arrow switches local branches. Network options arrows: Fetch chooses remote/all; Pull remote branch → current branch defaults to Rebase, with Merge as the alternative. Optional auto-stash saves/reapplies tracked changes (untracked stay in place; staging may change). Push chooses source/remote/destination with optional tags/upstream. Fixed-size close-only options are immediately editable from cached local refs/remote names; opening runs no Git queries. Use Fetch for new remote branches or Refresh after external configuration changes. Execution resolves the named remote using current Git configuration and validates the cached refs. Clicking the normal action closes options and starts Git. Follow the phase at top center, Cancel there or under Repository, and read results through Details → Operation. No extra approval checkbox; force options remain deferred.

Existing restrictions: Git rejects conflicting dirty/untracked paths; Shovel protects ignored collisions. Branch checkout still requires clean state and excludes sparse repositories; clean submodule superprojects switch without recursive checkout. Pull respects existing sparse patterns and updates superproject pointers without updating/stashing submodule contents. Active sequences block new branch/network commands; recovery controls are available separately. Custom SSH/multiple remote URLs and force remain unsupported. Hooks are preserved. [Action details](docs/architecture.md#explicit-git-actions-f2a-080). Rebase/merge/stash conflicts keep recovery state and original output. Resolve and stage each file in Local Changes, close that modal, then Continue from the banner/sidebar or Repository > Recovery. Skip/Abort confirm their scope. One last result/output remains in memory on its repository tab, with HTTP(S) credentials redacted. Cancel is not undo; fresh history/count inspection finishes before another action/tab/window close. Main-window auth prompts survive options closure; refresh failure is distinct from Git outcome.

## Headless timing reproduction

Opt-in discard profiling: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-discard` (text selections amid assets) or `--measure-discard-lfs` (hydrated assets; installed Git LFS required). `--measure-discard-lfs-trace` adds in-memory Trace2 child timing (extra overhead; overlapping lifetimes are not CPU time). Only allowlisted child categories and numeric durations are printed; raw trace is not saved. Each generates an isolated 2,000-text-file / 1,001-branch / 64×4-MiB asset fixture, reports three single-file and three batch samples with validation/command/reconciliation timings, and verifies exact bytes plus removed UI rows. Creation and final byte verification are excluded from operation timing. Fixtures remain under ignored artifacts/discard-*; this is not desktop p95 or a live-repository discard.


Recovery checks: `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-recovery`. Uses isolated disposable conflicts, hooks and offscreen sidebar controls; no visible launch or live remote. [Recovery workflow and limitations](docs/product.md#conflict-recovery-0170).

After build:

```powershell
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-history
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-large-history
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-actions
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-working-copy
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --measure-working-copy-assets
```

First: three samples/isolated 2,500 commits, separate settings/discovery/inspection/ref/page/graph/format/process timings, first/warm offscreen construction. Second: packed 100k commits/1,201 refs, first/last page before/after fixture-only commit-graph/packed-ref maintenance. App never maintains user repos for a benchmark. Regular checks cover auto-start/tab/refresh/render timing. Outputs artifacts/history-benchmarks; [roadmap](docs/roadmap.md) records comparisons. Not cold desktop/Windows 10/p95 certification.

Third: seven samples on an isolated 2,500-commit/one-remote fixture; separate cached editable-dialog layout, post-submission preparation/subprocess cost and prepared-dialog layout. Writes artifacts/action-benchmarks. First construction includes WPF/JIT initialization; later samples are warm. No window is shown, no network fetch or user repository is read.

Fourth/fifth: three stage/unstage samples on a disposable 2,000-file/1,001-branch fixture. The asset variant adds 64 unchanged 4 MiB assets and invalidates their stat cache each sample. Reports validation/command completion, fresh affected rows, then selected diff, plus subprocess names/durations. Wider background scans are excluded from the new target-row measurement. Writes artifacts/working-copy-benchmarks; no content/arguments in timing output. No reboot or OS cache flushing; not desktop responsiveness or p95 evidence.

## Build a local preview

```powershell
powershell -NoProfile -File .\scripts\Build-Preview.ps1
```

Local-only command: restores/builds/checks/publishes **Build/<version>-win-x64/Shovel.exe**, with helper/files/START-HERE/SHA256SUMS/changelog. Keep files together; runtime needed, SDK not needed to run. Unsigned local folder, no installer/updater/hosting/keys/push.

Build ignored/untracked; never overwrite an existing version. Partial publish stays in ignored artifacts. Increment Directory.Build.props and changelog once/delivery. User-requested 0.3.1 old-preview cleanup was one-time; subsequent versions retained. No additional prerequisite missing on this development host.

## Local data and recovery

| File | Content/handling |
| --- | --- |
| settings.json | Schema 6: layout/up to 20 tabs/order/selection/appearance/tool paths/legacy scope/identity; All Commits written since 0.7.0. No credentials/history. Schemas 1–5 migrate without consent; stamps saved after successful reads. Invalid/inaccessible/newer/unsupported or externally changed file is not overwritten. |
| history-view.json | Rebuildable last-selected history page (commit/ref/author/path metadata, selection and scroll), schema 1, 200 rows / 4 MiB. Clean-shutdown writes only; delete while closed to clear. Never authorizes mutations. |
| settings.json.bak | Previous valid bytes, including pre-migration data; no automatic restore. |
| diagnostics.jsonl / .1 | UTC + typed codes; draft retry/failure adds only operation, storage stage and numeric native error; ~256 KiB each, bounded queue/best effort including fatal exit; no raw output/exceptions/paths/content. |

Close app, copy settings/backup first; inspect schema before restoring or move invalid file aside for defaults. Older previews that do not support this schema refuse writes; downgrade via separate profile/compatible backup. External-edit check is not a transaction with arbitrary editors; never edit during save.

## Current limits

Current identity supports root on local fixed drive/no reparse components; UNC/removable/junction/symlink/subfolder opening unsupported. Missing/replaced targets require explicit reopen without trust modal; identity is not an in-place-hostile-change/atomic-race guarantee. [Read audit](docs/architecture.md#automatic-local-history).

History uses one read chain/max two children; the modal has bounded cancelling status/diff chains. Mutations cancel old reads without waiting for their drain; close drains them. Bare repos rejected, linked actions coordinated within Shovel, external clients independent. Shallow ends distinct, missing partial objects fail without fetch. Detected merge/rebase/cherry-pick/revert sequences support recovery; mailbox/ambiguous state remains unsupported.

History: 4 MiB/read, 64 lanes, 15 s/command, 45 s/read chain, 128 cursors/tab; errors explicit, no false truncation. Deep --skip can time out. Full selected fields within read bound, abbreviated rows. The [shared Dark/Light themes](docs/theme.md) apply to the client area; Shovel title bars use native palette-aware styling (exact colors on Windows 11, best effort on Windows 10); pickers/message boxes remain system-owned. Dark/Light selection is available under Appearance; further theme polish remains deferred. Pending: arbitrary-line staging, image/text side-by-side diffs, broader mutations/filtering, auth routes, locales/accessibility/DPI, representative performance and installed updates. See TODO for priorities, roadmap for measured limits.

## Windows installer and updates

Master includes the updater and the latest Git workflows. GlebHmarsky/Shovel is the single public source and release repository under MIT. **Every delivered build is a release**, available through Help → Shovel updates.

[Download the latest Windows installer](https://github.com/GlebHmarsky/Shovel/releases/latest/download/Shovel-Setup.exe). Already installed: use Help → Shovel updates.

Increment `Directory.Build.props`, add release notes to CHANGELOG, commit the changes on master, then run:

```powershell
pwsh -NoProfile -File scripts/Build-Release.ps1
```

This runs smoke verification once, builds the Windows x64 self-contained installer, signs the feed with the existing `%USERPROFILE%/.shovel-signing/shovel-update.key`, uploads via the current Git Credential Manager login, and verifies an anonymous download using Shovel's production update source. GitHub CLI is not required for stable releases. The routine Beta installer is `Build/<version>-installer-win-beta/Shovel-Setup.exe`. No private key or login token is uploaded.

The command requires clean master, pushes the source, and tags the release at the exact packaged source commit. It refuses changed existing builds and published version reuse. An upload failure leaves a draft for inspection and does not advance latest. Fix the cause before retrying; never overwrite a published version. `Build-Preview.ps1` is an explicit local-only development tool, not completed delivery. `Build-Installer.ps1 -LocalOnly` remains an unconfigured offline fixture.

[Distribution and recovery](docs/distribution.md) covers install locations, permanent key backup, schema-compatible repair, and validation limits. Windows publisher signing is separate and still unset. Installed UI/restart acceptance remains distinct from headless and network verification.

Version 0.32.1 switches installed update checks to this repository. Older installed builds can receive that migration version from the archived shovel-releases endpoint; after updating, they use only Shovel. The offline Local Test fixture still requires a regular installer.
