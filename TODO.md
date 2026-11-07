# Implementation checklist

Updated 2026-11-07. Single prioritized task list; [roadmap](docs/roadmap.md) holds completed milestones/evidence. P0 = correctness prerequisite; P1 = next delivery; P2 = later. Later user requests can change order without reapproving a whole historical phase.

## Current phase

### Entire Rust application — active, not delivered

Owner request 2026-11-05. Parity baseline: C# **0.47.0**, commit **2175458**. Later sections describe C# deliveries, not completed Rust work. Current product/requirements supersede historical behavior (especially command lifetimes, quiet feedback, remembered network flags and approved updates). Every unchecked item below remains required before claiming parity; backend-only tests do not complete UI rows. C# deferred/unimplemented extensions remain deferred rather than being misrepresented as existing functionality.

Current partial implementation includes history/comparison/Commit, Rust askpass and Fetch/Pull/Push, local branch/upstream management, reviewed checkout, confirmed history Reset/Revert, native sequence Continue/Skip/Abort, saved-stash inspection/comparisons/Create/Apply/Pop/Drop, inline native-side/text/tool conflict resolution, and captured local-file Open/Explorer/Stage/Unstage/Copy/confirmed Discard with selected-LFS hash reconciliation selected-path Stash with unrelated-index preservation, configured External Diff, root Ignore/LFS rule editing, and atomic binary patch export through an asynchronous Windows save picker, and captured-tip file History/Blame with rename paging and real revision comparisons. Focused evidence and queue/authentication/UI limits are in [roadmap](docs/roadmap.md#independent-rust-rewrite--active-2026-11-05). R58–R64 remain unchecked: native menu/keyboard/appearance, persisted pane geometry and all remaining scenarios are not certified. Continue the remaining parity rows before packaging/publication.

#### Current owner delivery — workspace and installer preview (2026-11-07)

- [x] Keep Branch beside the central repository display and Open in at the right; expose Activity from the display and View.
- [x] Show bounded, left-aligned Branches/Remotes/Tags/Stashes groups; locate selected refs across 200-row pages and reveal them in history. Stash inspector selection also requests history navigation.
- [x] Keep changed-file paths single-line with tooltips and an adjustable vertical divider; smooth graph bends.
- [x] Replace network arrow popovers with remembered-options dialogs; dismissal saves choices without executing a command.
- [x] Add categorized working appearance/scale/history-label/diff/Git/identity/tools/update preferences, cancellable automatic fetch and explicit repository tool handoff.
- [x] Add Rust-only signed update transport and package verification, offered download/retry/cancel, persisted channel floors and guarded restart.
- [ ] Generate and verify the 0.49.1 Rust preview installer and record its artifact receipt. No merge or public feed publication.
- [ ] Complete the broader parity rows below. Settings avatar providers, full sidebar/submodule workflows, full activity categorization, installed Beta-to-Beta update, live UI/authentication and paired C#/Rust measurements remain open. This preview is not full C# parity.

#### P0 — isolation, architecture and delivery

- [x] **R01** Create separate worktree on `codex/rust-rewrite`; preserve clean C# master, source, historical branches and existing releases.
- [ ] **R02** Establish `codex/rust-beta` and `codex/rust-stable` release branches; Rust-only package/executable/profile/shortcut/update identities, Beta default and explicitly approved Stable promotion. Local release branches created at foundation `01c7593`; installer/shortcut and public feed establishment remain pending.
- [ ] **R03** Pin Rust/MSVC dependencies and lockfile; compile desktop UI and askpass as Rust executables. No .NET runtime, C# application subprocess, browser UI wrapper or runtime bridge in Rust packages. Preserve license/icon/notices.
- [ ] **R04** Adapt Build-Release/preview/installer/publisher/probe dispatch for Rust branches with clean-source checks, source fingerprint, exact release-branch/tag commit and immutable versions. Preserve original C# scripts/feeds on master.
- [ ] **R05** Signed Rust Beta/Stable metadata with pinned existing public key, RSA-PSS verification, size/hash/package/product/channel/architecture checks, per-channel replay floors, bounded HTTPS and no credentials. Reject C# feeds/packages and cross-channel substitution.
- [ ] **R06** Publish immutable Rust Beta installer/assets before signed feed alias. Keep GitHub latest, C# `beta` alias and C# release assets unchanged. Reserve Rust Stable with no publication until explicit owner approval.
- [ ] **R07** Install two Rust Beta versions under isolated Rust identity; exercise actual native check/download/verify/apply/restart and prove version advance plus settings/drafts retention. Test failed/blocked update, repair and uninstall while preserving C# installation/data. Record actual evidence, not transport-only claims.
- [ ] **R08** Rust-native update UI: launch/manual/channel/periodic discovery, explicit offered-version download/restart approval, progress/error/cancel/retry/postpone, channel switch invalidation and no downgrade. Guard restart across profiles/sessions, accepted work, tools and repository state; preserve verified retry.
- [ ] **R09** Repository ownership and scheduler: initialization independent of views, duplicate coalescing, ordered conflicts, individual waiting removal, failure clears only own waiting work, accepted commands survive tab/view close/deadlines, failed Commit reopening, bounded retained owners/results; only auto-fetch preemptible.
- [ ] **R10** Windows process host: literal Unicode arguments/environment, exact Git executable discovery/custom selection, inherited-handle containment and job ownership, concurrent bounded stdout/stderr, stdin, descendant drainage and separate read/user admission lanes. Preserve native hooks, filters, signing, SSH/GCM/config; no silent helper bypass.
- [ ] **R11** Repository identity: exact root/git/common directories and file identity; linked-worktree mutation mutexes, replacement detection, stale captured-target validation, no trust gates, no automatic cleanup/retry/rollback.
- [ ] **R12** Rust localization catalogue from current resources; whole-message formatting, original Git output, semantic accessibility names, RTL/direction isolation, cultural dates and noncolor outcome text. Bounded allowlisted diagnostic metadata only; session transcripts redacted and never persisted.

#### P1 — desktop workspace and persistence

- [ ] **R13** Native Rust main window: integrated File/View/Repository/Window/Help caption, OS move/resize/min/max/system menu, same icon/layout/toolbars and minimum sizes, centered repository/branch/static operation feedback. No removed timers/animated activity indicators.
- [ ] **R14** Dark/Light palette parity, Windows high contrast and native dark dialog chrome, shared hover/selection/focus/disabled states; 70–200% saved scale and Ctrl+Plus/Minus for all windows; DPI/accessibility/keyboard review.
- [ ] **R15** Equal-width tabs; open/restore/selected-first loading, 20 saved tabs/order/selection; context Refresh/Move/Close/Close others, middle-click close and keyboard selection; full-window full-slot drag preview, neighboring movement, Escape restore, no repository deletion.
- [ ] **R16** Persist main/Commit/all resizable dialog geometry, proportional panes, sidebar expansion and bottom-tab choice; preserve legacy geometry migration and close-only resizable Commit caption.
- [ ] **R17** Rust profile storage independent from C# with compatible settings import/copy, all schema-6 preferences and bounded older-schema migration; preserve unknown/newer/corrupt files, atomic backup/replace, competing-writer detection, failed-save retention and profile mutex. Never redirect a C# updater via shared settings.
- [ ] **R18** Persistent per-worktree drafts: identity/revision/competing-save protection, short sharing/lock-only retries off-thread, recovery copies, restore after editor display/newer typing wins, clear only submitted text after success and retain failed draft/index.
- [ ] **R19** Selected verified startup preview with bounded identity/page/selection/scroll cache, explicit last-known label and no mutation readiness; session 3-page cache/32-MiB budget/30-second activation refresh/generation rejection, no inactive history preload or status polling.
- [ ] **R20** Settings categories Appearance/History & avatars/Diff/Git/Git identity/Tools/Updates: open with no preparation; edit/save/apply/cancel and failed-write retention. Bounds and defaults match baseline, including one-time 10→60-second migration.
- [ ] **R21** Explicit global name/email Load/Apply through selected Git; reviewed-value checks, native locks, partial-write reporting without rollback or loss of other config. Cancel does not undo applied identity.
- [ ] **R22** Colored Unicode initials/anonymous GitHub/Gitea subpath+exact-email fallback/Gravatar avatars; same defaults and preferences, async bounded deduplication/cache/concurrency/bytes/dimensions, stale consumer cancellation and no credential forwarding.
- [ ] **R23** Activity user/background/all-repository filters, 128 bounded entries/active exemption, actual command/output details and individual waiting removal; retained static outcomes and explicit Reopen Commit. User failure/warning attention and inactive taskbar flash; background stays quiet.
- [ ] **R24** Shutdown/save/error handling: refuse exit during accepted work, settled conflicts alone do not block ordinary exit; drain obsolete reads/background work, save layouts/drafts, preserve failures and explain retry/exit-without-save. No surprise autostart/tray behavior.

#### P1 — repositories, history, graph and comparisons

- [ ] **R25** Open folders/custom Git; real metadata for ordinary/unborn/detached/linked/bare/invalid/missing/replaced/sequence/shallow/partial/sparse/LFS/submodule states, exact paths and capability reasons. Automatic selected history without trust prompts or network.
- [ ] **R26** Initialize new/existing populated directories with default/explicit branch and templates; zero opening preparation, no implicit commit/staging/remote, preserve partial/native results and open actual repository after submission.
- [ ] **R27** Clone URL/absolute path with optional branch/tag; literal arguments/auth/progress/native checkout/filter/hooks, reject nonempty target, preserve partial output and open valid resulting repository without retry/deletion.
- [ ] **R28** All Commits traversal: branches/remotes/tags/HEAD/stash bases, detached/unborn/peeled tags/noncommit omission, exclude unrelated reflog/unreachable history. 200 visible rows, Older/Newer/First/View/range and page-edge/Ctrl+Page navigation without total ceiling.
- [ ] **R29** Stable graph lane/color/cursor/64-column bounds, hollow merge markers, old-base stash adjacency/compact same-base leaves, fold only stash internals not independently reachable commits, correct cross-page topology and no duplicate/gap navigation.
- [ ] **R30** Headerless viewport virtualization, graph/refs/description/author/date/hash ordering, ref-left/right preference, full tooltips and complete selected metadata within bounds. Same-tip upstream badges/current mark and exact raw-author peer background; selected/high-contrast semantics preserved.
- [ ] **R31** Fresh/stale/pending/failed/empty/shallow/end/history limits, retain useful rows/selection/scroll on failed paging or refresh; unchanged refs keep page/row identity, changed refs restart appropriately; no implicit fetch, signatures, textconv or external diff during reads.
- [ ] **R32** Real local-upstream ahead/behind after history; zero distinct from unavailable, detached/unborn/missing/shallow/stale cases. Preserve Pull-upstream counts even with separate Push destination.
- [ ] **R33** Cached sidebar branches/remotes/tags/stashes/submodules, expansion/scroll/tooltips, delayed secondary reads; single-click locate across pages, double-click/Enter switch and concrete context actions with captured targets.
- [ ] **R34** Commit/Changes lower panes: author/committer/original-offset dates/full hash/parents/refs/full message, explicit merge-parent comparison, actual changed files; retain chosen tab and labelled old comparison until generation-matched result arrives.
- [ ] **R35** Unified numbered/colored text diffs, addition/deletion/rename/EOF/EOL and binary/oversized/missing boundaries; side-by-side aligned old/new with context/blank padding/shared scrolling/resizing/wrapping, default preference/font 9–24 and per-comparison toggle.
- [ ] **R36** Binary/LFS Old/New cards: extension/icon/logical local sizes/exact bytes, absent vs zero length, no implicit hydration/helper and no pointer-as-asset display; native immutable historical file/tree content.

#### P1 — local changes and commit

- [ ] **R37** Modal Commit opens immediately with cached lists/draft, schedules own scan after display and does not cancel initialization; separate staged/unstaged/untracked/conflicts, unique count, selection/captured menus, F5, useful stale rows and no polling.
- [ ] **R38** Selected/all Stage/Unstage and Shift variants; exact index/worktree semantics for initial/partial/rename/delete/binary/Unicode/dash paths. Semantic index validation ignores stat refresh while rejecting real staged changes; native locks/hooks and stale content checks.
- [ ] **R39** Whole-hunk multi-selection Stage/Unstage with original hunk identity, CRLF/EOF, offset/disjoint changes, index/HEAD/file/config/attribute validation; no whole-file fallback for unsupported filtered/binary/types. Exact unrelated content preservation.
- [ ] **R40** Subject/body Commit, initial/ordinary/amend/merge, target-specific amend confirmation, Ctrl+Enter/Ctrl+Shift+Enter, Shift Commit+Push. Close on submission, repository-owned execution/reconciliation, separate commit-success/push-failure, preserve failed drafts/index.
- [ ] **R41** Fast targeted file-result reconciliation before coalesced full scan, preserve same-path selection/unrelated rows; late diff/status generations cannot block or overwrite newer actions, failed wider refresh explicitly stale.
- [ ] **R42** Local file menu/shortcuts: captured selected/all stage commands, Enter, Ctrl+Shift+Alt+S, Copy relative/full path, Ctrl+C/Shift+C, Open/Explorer and Ctrl+Shift+Alt+O; no menu-opening Git/discovery, delay selection diff until menu closes.
- [ ] **R43** Confirm Discard exact paths/rename endpoints: unstaged checkout-index preserving staged work/native smudge/LFS/index hooks without post-checkout, staged reset index+worktree to HEAD, selected untracked/unborn deletion. Reject unsupported conflict/submodule/reparse, stale changes; retain partial outcomes.
- [ ] **R44** Standard LFS exact-content status/discard reconciliation with local SHA-256 pointer semantics, same-size/time edits, custom filter/extension fallback, missing-object failures and no optimistic clean result/hook bypass.
- [ ] **R45** Selected stash retains unrelated index/worktree, optional message and selected untracked; real saved entry and partial cleanup outcomes. Configured External Diff per staged/unstaged/untracked side, native tool lifetime/errors.
- [ ] **R46** Root Ignore literal paths/extensions for untracked and LFS Track/Untrack root rules, exact escaping/native attributes/filter requirements and override reporting; no migration/restaging/install. Save selected binary patches atomically through picker, preserve existing output on failure.

#### P1 — branches, remotes, network and tools

- [ ] **R47** New branch from captured commit/start, optional checkout without inherited upstream; reviewed branch switching and detached checkout, preserve native worktree/sequence/overwrite protections.
- [ ] **R48** Checkout confirmation displays before scan, clean vs dirty choices Keep/Stash+reapply/Discard tracked; exact target and reviewed staged-state checks, untracked/index restoration or retained stash on failure, native checkout hooks/LFS and no recursive submodule mutation.
- [ ] **R49** Delete local branch with actual tip/target confirmation, native merged/current/linked-worktree guards; Set/Unset upstream from existing fetched refs with stale configuration validation and no implicit network/force deletion.
- [ ] **R50** Add/edit/remove remotes from cached config; fetch/push URL fallback and unsupported multiple/inherited/credential-bearing values, exact removal confirmation; preserve local tips/index/worktree, report partial edits and refresh without server contact.
- [ ] **R51** Fetch/Pull/Push split buttons with direct remembered flags, preparation-free options, choices saved on dismissal, explicit branch-bound destinations and implicit current Git config. Missing/ambiguous/deleted targets explained without silently substituting.
- [ ] **R52** Fetch one/all with authentication and native tracking, no recursive mutation/maintenance; independent history/Commit/stage/unstage/commit during fetch. Pull consumes FETCH_HEAD under correct serialization and validates current configured remote/refspec.
- [ ] **R53** Pull rebase/merge and explicit tracked auto-stash; divergent/untracked/ignored/sparse/submodule semantics, conflicts including zero-exit stash reapplication, native hooks/signing and retained sequence/recovery. No automatic cleanup/retry.
- [ ] **R54** Push configured supported simple/current/upstream/tracking destination, explicit remote/source/target, optional all-tags/set-upstream, no implicit force; actual rejection/results/reconciliation and preserved target when queued.
- [ ] **R55** Optional one-minute automatic Fetch, interval 10–3600, fresh interval after preference changes, max two/oldest due, no prompt or local-status polling, user priority/preemption and background-only results; disabling drains only automatic work.
- [ ] **R56** Rust askpass helper: per-operation protected endpoint/nonce/server validation, bounded requests/replies/deadline, masked owned prompt/host confirmation, no credential/prompt persistence. Preserve shared GCM/SSH agents, ordinary host verification and existing unsupported-input explanations.
- [ ] **R57** Explorer/Rider/VS Code/Git Bash handoffs and shortcuts; discovery only after explicit action, exact arguments/root project preference, configured paths/browse fallback, launch failures and external-session restart guidance; do not kill handed-off tools.

#### P1 — stashes, history actions, conflicts and file inspection

- [ ] **R58** Stashes owned resizable inspector with cached immediate list/message then async refresh, full saved metadata and tracked/index/unstaged/untracked comparisons; stable hash selection after ordinal changes, stale results/retained content.
- [ ] **R59** Create optional untracked, Apply/Pop optional index restoration, exact-target confirmed Drop; pop removes only after actual success/no conflicts/unchanged list, retain partial outcomes/saved data. Inline history/sidebar quick actions use same real workflows.
- [ ] **R60** History context menu captures immutable row SHA with zero preparation; full SHA copy, detached Checkout, confirmed Soft/Mixed/Hard Reset with correct index/worktree outcomes, Revert including explicit merge mainline and native signing/hooks/conflicts.
- [ ] **R61** Sequence detection/banner/Resolve/Continue/Skip/Abort for supported merge/rebase/cherry-pick/revert states; exact reviewed target and destructive confirmation, unresolved guard, native message/exec behavior, reopen observes actual state. Stash conflicts never invent a sequence.
- [ ] **R62** Inline conflict stage 1/2/3/source provenance for merge/rebase/stash/deletion/binary/LFS, named sides/check selection only, Resolve applies/stages explicit side or edited result, no automatic sequence continuation.
- [ ] **R63** Conflict UTF-8/BOM UTF-16 LE/BE editing with original byte order/BOM/EOL, 512-KiB/20k-line/16k-column bounds, malformed Unicode/residual marker rejection, stale index/config/attribute/file validation, retain edits and saved result on stage failure, unsaved-close confirmation.
- [ ] **R64** External merge tool saved/default/named/path/custom command chooser, no selection mutation/discovery, per-process overrides/native shell variables, exact unresolved-index verification, retained errors/backups and detaching-tool limits.
- [ ] **R65** Historical file menu Open/External Diff/Explorer/Reset File To/Blame/History/File Tree/LFS/Save as/Copy paths; immutable selected commit/explicit parent/HEAD sources including rename/delete; confirmed worktree-only restore, native filtered export up to 64 MiB, index/unrelated preservation.
- [ ] **R66** First-parent file History with rename following and 200-row pages, real selected file comparisons, captured-tip Refresh; committed Blame/Timeline fields and content, bounded binary/unsupported/missing states, no network and retained failure rows.
- [ ] **R67** Selected submodule Open/Update/Sync actual indexed target/config, dirty overwrite/native authentication protections, no force/recursion/remote-latest/add/remove/deinit; parent/child ownership and useful outcomes.

#### P1 — parity verification, performance and final publication

- [ ] **R68** Trace every baseline production source/resource/UI command to above acceptance rows; inventory maintained at `docs/rust-source-inventory.json`. Review all 194 baseline files and relevant original tests; record coverage gaps explicitly.
- [ ] **R69** Disposable isolated-config real-Git differential fixtures for above operations/failures/state preservation, held hooks/initialization/concurrency/queues, literal paths and locks; compare exact HEAD/index/worktree/config against baseline, not only exit codes.
- [ ] **R70** Rust UI interaction/layout verification against baseline at normal/minimum sizes, Dark/Light/high contrast/scale/DPI/keyboard/menus/dialogs/drag/virtualization. Verify real wired controls; offscreen evidence is not desktop acceptance.
- [ ] **R71** Explicit <=30-check smoke under 120-second target/180-second suite/30-second check caps, fail first, matching preview/source fingerprint and native package validation; optional detailed parity suites selected independently, not hidden inside smoke labels.
- [ ] **R72** Equivalent paired C#/Rust startup-to-visible-first-rows (cold/warm defined), idle/peak process-tree working set/private bytes, history first/later/cache/refresh, stage/unstage/Commit and single/batch LFS discard. Same machine/Git/fixture/scope/200-row page/profile/filters/build mode; raw samples and medians/variation, no assumed speedup.
- [ ] **R73** Feed signature tamper/replay/wrong-product/wrong-channel/architecture tests, anonymous production installer/package verification and C# endpoint/assets unchanged before/after. Actual installed Beta A→B proof, Rust Stable absence/approval gating and retained settings/drafts.
- [ ] **R74** Final parity audit, roadmap dated evidence/limits, Rust changelog/version/notices, clean commits and signed downloadable Rust Beta publication through Build-Release. Keep goal active while any current functionality, mandatory update rehearsal or measurements are missing.

### C# baseline history (reference only)

- [x] **P1 - Workspace corrections and refactor (0.47.0 Beta).** Compact topological stashes, reviewed checkout choices, remembered network options, persistent sidebar/window/pane state, close-only Commit, semantic index validation and compact attention. Shared state/locks and obsolete timer cleanup; 51 focused checks plus disposable LFS measurements. [Evidence and limits](docs/roadmap.md#workspace-corrections-and-shared-state-0470-beta).
- [x] **P1 - Verify and deliver 0.47.0 Beta.** 51 focused checks and 26 smoke checks in 65.5 s; matching installer/signed feed, all ten public asset digests and anonymous production download verified. Stable stays 0.39.1. No desktop control or installation. [Evidence and limits](docs/roadmap.md#workspace-corrections-and-shared-state-0470-beta).

- [x] **P1 - Repository-owned operations (0.46.0 Beta).** Independent initialization, retained tab/view owners, ordered conflicting requests, individual waiting removal, background Commit and failed-draft reopening. [Behavior and limits](docs/product.md#repository-owned-work-0460-beta).
- [x] **P1 - Deliver 0.46.0 Beta.** 58 focused checks and 26 smoke checks in 68.8 s; matching installer/signed feed, all ten public asset digests and anonymous production download verified. Stable stays 0.39.1. [Evidence](docs/roadmap.md#repository-owned-operations-0460-beta).
- [ ] **Deferred - Running user-command cancellation.** Ask the owner again before implementing. Current policy permits removing waiting requests and preempting automatic fetch only. Hung hooks/tools can therefore prevent exit.

- [x] **P1 — Categorized settings and author pictures (0.45.0 Beta).** Seven left-side categories expose existing preferences plus diff font/default, explicit global Git identity and configurable GitHub/Gitea/Gravatar pictures with immediate colored initials. 55 focused checks passed. [Behavior and limits](docs/product.md#categorized-settings-and-author-pictures-0450-beta).
- [x] **P1 — Deliver 0.45.0 Beta.** 55 focused checks and 26 smoke checks in 70.0 s; matching installer/signed feed, all ten published asset digests and anonymous production download verified. Stable remains 0.39.1. [Evidence and limits](docs/roadmap.md#categorized-settings-and-author-pictures-0450-beta).

- [x] **P1 — Quiet fetch and usable workspaces (0.44.0 Beta).** One-minute default/migration, background-only logging, independent working-copy use during fetch, user action priority and repository/context-menu availability.
- [x] **P1 — Historical-file actions and history/activity UI (0.44.0 Beta).** Screenshot context menu with real scoped actions, saved ref-label placement and visible command transcripts.
- [x] **P1 — Deliver 0.44.0 Beta.** 55 focused checks and 26 smoke checks in 66.1 s; signed-feed installer published after verified draft-upload recovery; anonymous full-package verification passed. Stable remains 0.39.1. [Evidence and limits](docs/roadmap.md#background-fetch-and-history-actions-0440-beta).

- [x] **P1 — External merge-tool configuration (0.43.0 Beta).** Saved Git-default/named executable/custom-command settings, native per-invocation launch and 35 passing focused checks. [Behavior](docs/product.md#external-merge-tool-configuration-0430-beta).
- [x] **P1 — Deliver 0.43.0 Beta.** 26 smoke checks in 58.9 s; matching installer, signed feed and anonymous full-package verification passed after CDN propagation. Stable remains 0.39.1. [Evidence](docs/roadmap.md#external-merge-tool-configuration-0430-beta).

- [x] **P1 — Deliver 0.42.0 Beta.** 21 focused checks and 26 smoke checks in 62.3 s; matching installer, signed update feed and anonymous full-package verification passed. Stable remains 0.39.1. [Evidence](docs/roadmap.md#auto-fetch-settings-0420-beta).

- [x] **P1 — Queue interactions and remove timer feedback (0.41.0 Beta).** Unique toolbar requests retain repository/branch and serialize conflicting work; timers/animations removed, full-window/full-slot drag feedback, viewport profiling and earlier branch context. [Evidence and limits](docs/roadmap.md#queued-actions-and-quiet-feedback-0410-beta).
- [x] **P1 — Deliver 0.41.0 Beta.** 26 smoke checks in 36.8 s; matching installer, signed update feed and anonymous full-package verification passed. Stable stays 0.39.1. [Evidence](docs/roadmap.md#queued-actions-and-quiet-feedback-0410-beta).

- [x] **P1 — Repository workflow requests (0.40.0 Beta).** Per-repository operations/results, stash lanes, successful Commit close/Ctrl+Enter, proportional panes, ten-second auto-fetch, F5 local refresh, attention logs/taskbar API and left conflict actions. Focused evidence and limits in [roadmap](docs/roadmap.md#repository-workflows-and-attention-0400-beta).
- [x] **P1 — Deliver 0.40.0 Beta.** 26 smoke checks in 32.6 s; matching installer, signed update feed and anonymous full-package download verified. Stable stays 0.39.1. [Evidence](docs/roadmap.md#repository-workflows-and-attention-0400-beta).
- [x] **P2 — Auto-fetch settings (0.42.0 Beta).** File → Settings persists application-wide enablement and 10–3600-second interval, applies scheduling changes, cancels background-only work, preserves manual operations and reports save failures. [Behavior](docs/product.md#auto-fetch-settings-0420-beta).


- [x] **P1 — Complete representative history measurement (0.39.1).** Corrected stale harness assumptions; merge-heavy/100k/million profiles and five-minute idle passed. Cached publication and full comparison drain are separate; no scope/page reduction. [Evidence](docs/roadmap.md#representative-history-validation-0391).
- [x] **P1 — Deliver 0.39.1 Beta and Stable.** Owner-approved Stable and Beta published from the same source; 26 smoke checks in 43.3 s and both signed anonymous downloads passed. Later routine deliveries remain Beta-only. [Evidence](docs/roadmap.md#representative-history-validation-0391).


- [x] **P1 — Side-by-side diffs and update approval (0.39.0).** Aligned Old/New text in history and Local Changes with whole-hunk mapping; immediate discovery and one approval before download/restart.
- [x] **P1 — Deliver 0.39.0 Beta.** 8 QOL, 11 hunk and 17 update checks; 26 smoke checks in 58.6 s; signed publication and anonymous full-package verification passed. Stable remains 0.35.0. [Evidence](docs/roadmap.md#side-by-side-comparisons-and-approved-updates-0390-beta).
- [ ] **P2 — Desktop acceptance (owner-only).** Owner confirmed the update flow works and forbids further computer control. Physical shortcut/drag and taskbar evidence remain unverified by the agent; do not resume desktop automation without a new explicit request.
- [ ] **P2 — Avatar provider extensions/acceptance.** Verify live provider/server behavior when authorized; authenticated private profiles, additional providers and per-repository Gitea overrides remain outside the current anonymous session-cache implementation. Failed/unavailable pictures retain initials.


- [x] **P1 — Workspace refinements (0.38.0).** Combined Commit overview/files/diff, retained bottom tab, saved keyboard scale, title and drag feedback; explicit shortcut-matching process identity.
- [x] **P1 — Deliver 0.38.0 Beta.** 7 QOL and 10 committed-file checks; 26 smoke checks in 42.4 s; signed Beta feed and anonymous full-package verification passed. Stable remains 0.35.0. [Evidence](docs/roadmap.md#workspace-refinements-0380-beta).

- [x] **P2 — UTF-16 conflict editing (0.37.0 Beta).** BOM-marked LE/BE sources and result encoding in the existing pane, exact encoded writes, strict Unicode/byte bounds and preserved native side/tool fallback. [Behavior and limits](docs/product.md#daily-use-interactions-0310).
- [x] **P1 — Deliver 0.37.0 Beta.** 17 conflict checks and Dark/Light normal/minimum review; 26 smoke checks in 53.3 s; matching preview/installer, signed update feed and anonymous full-package verification. Stable remains 0.35.0. [Evidence and limits](docs/roadmap.md#utf-16-conflict-editing-0370-beta).

- [x] **P2 — Refactor selected LFS discard verification (0.36.1).** Share exact-byte hashing, remove redundant raw status scans for supported selections, retain native fallback and concurrent edits. Same-host median: one asset 1.215 → 1.104 s; 64 assets 3.691 → 2.009 s. [Evidence and limits](docs/roadmap.md#lfs-discard-refactor-0361-beta).
- [x] **P1 — Deliver 0.36.1 Beta.** 11 LFS and 6 discard checks; 26 smoke checks in 48.6 s, matching preview/installer, signed update feed and anonymous full-package verification. Stable latest remains 0.35.0. [Evidence and limits](docs/roadmap.md#lfs-discard-refactor-0361-beta).

- [x] **P1 — Deliver 0.36.0 Beta.** Inline external merge-tool chooser; 10 focused conflict checks and 26 smoke checks (45.7 s), signed Beta publication and anonymous package verification. Stable remains 0.35.0. [Evidence and limits](docs/roadmap.md#external-merge-tool-chooser-0360-beta).

- [x] **P1 — Stable/Beta channels (0.35.0).** Persisted selector/default Stable, isolated signed feeds and package validation, pending-update invalidation, no cross-channel downgrade; routine Build-Release defaults to Beta. Explicitly publish the common baseline to both; future Stable releases require owner approval.
- [x] **P1 — Deliver common 0.35.0 baseline.** 17 updater checks; 26 smoke checks in 33.1 s; same source/version published as Stable release and Beta prerelease. Both signed anonymous downloads verified; Beta left Stable latest unchanged. [Evidence](docs/roadmap.md#stable-and-beta-channels-0350). Installed channel-switch/restart acceptance remains with the owner.

- [x] **P1 — Automatic launch updates (0.34.0).** Immediately check/download/verify on each installed launch and automatically request the existing guarded restart. No delay, periodic-preference exemption, verification bypass or repeated restart loop. Blocked/cancelled/failed attempts remain usable. Installed restart acceptance remains in the rollout task.
- [x] **P1 — Deliver 0.34.0.** 16 updater checks and 26 smoke checks (32.9 s); Build-Release published the signed feed and installer on Shovel and verified an anonymous full-package download. [Evidence](docs/roadmap.md#automatic-launch-updates-0340).

- [x] **P1 — Clone and remote URL management.** Preparation-free Clone/Manage remotes forms; native checkout with filters/hooks, authentication/cancellation, partial-result retention and automatic history opening; cached fetch/push URL editing and confirmed local remote removal with identity/configuration checks and reconciliation. [Flow and limits](docs/product.md#clone-and-remote-management-0330).
- [x] **P1 — Deliver 0.33.0.** 11 clone/remote and 2 shared network checks passed; Build-Release passed 26 smoke checks in 31.8 s, packaged the exact committed source, published the signed update feed on Shovel and verified an anonymous full-package download. [Release evidence and limits](docs/roadmap.md#clone-and-remote-management-0330).

- [x] **P1 — Consolidate source and updates in public Shovel.** 0.32.1 is published and signed on the main repository; clean-master delivery tags the exact source commit. The old host is archived with one final migration release. Both anonymous download routes were verified; all future publishing is restricted to Shovel.


- [x] **P1 — Merge installer/updater into master and make releases the delivery default.** 0.32.0 retains current UI/Git features and settings compatibility; Build-Release packages, signs, publishes and verifies each delivery. Source and release hosting now share public GlebHmarsky/Shovel; old hosting is only a migration endpoint. See distribution for the command and roadmap for verification evidence.


- [x] **P1 — Daily-use QOL.** Retain file/commit comparisons while loading; preserve unchanged selected rows; navigate history/stashes to Commit changes; embed whole-file conflict selection/editing/staging in Commit; persist pane/window sizes; quiet tabs/arrows/badges and add bounded All/User/Background activity. [Behavior and limits](docs/product.md#daily-use-interactions-0310).
- [x] **P1 — Deliver 0.31.0.** Six QOL, eight conflict, eleven stash and ten commit-file checks passed; 26 smoke checks in 53.0 seconds, zero build warnings/errors, matching ignored preview/checksums and owning docs. Native input coverage, crash-time geometry and advanced merging remain limitations.

- [x] **P1 — Diagnose and fix EOM's slow LFS discard.** Retained Shovel output and Fork Activity Manager identify the command/hook difference; user confirmed Fork's unstaged-discard semantics. Native conversions/index hooks remain; hook files/branch/staged-discard behavior unchanged. Same EOM assets: old restore pipeline 33.4 s, fixed single/two-file selections 1.3–2.1 s, exact hashes and preserved stash. [Evidence and limits](docs/roadmap.md#lfs-discard-0301).
- [x] **P1 — Deliver 0.30.1.** Focused discard/LFS checks; 26 routine checks in 52.8 seconds, zero build warnings/errors, matching ignored preview/checksums and owning docs. New preview launched and restored EOM; desktop discard submission remains an automation limitation. Large-batch/cold-cache limits remain below.
- [x] **P1 — Conflict resolution.** Actual source labels, per-file side checkout/staging and UTF-8 result editing; merge/rebase banner with Resolve/Continue/Abort. Binary/LFS/deletion/rebase/stash and stale/failure paths covered by focused checks. Configured external tool works; tool chooser and additional encodings remain limits.
- [x] **P1 — Startup and daily-use fixes.** Bounded saved last-selected history preview; real stash history Apply/Pop/Drop; themed disabled tab strip and persisted drag reorder. Headless cache publication and offscreen theme checks passed; native desktop input remains unverified.
- [x] **P1 — Profile discard and correct LFS status.** Read-only authorized asset-repository inspection; disposable text/LFS single/batch measurements; overlapping validation and consolidated HEAD reads; restore filters/hooks preserved. Fixed false modified rows after hydrated LFS restores. See [results](docs/roadmap.md#conflicts-and-usability-0300).
- [x] **P1 — Deliver 0.30.0.** Focused conflict/usability/LFS/status/responsiveness checks and offscreen review; 26 routine checks in 55.6 seconds, zero build warnings/errors, matching ignored preview/checksums, owning docs and descriptive source commit. No interactive launch; LFS latency remains open below.

- [x] **P1 — History context actions.** Clicked full SHA; detached Checkout; confirmed Soft/Mixed/Hard Reset; Copy SHA and Revert with explicit merge mainline. Preparation-free menu, shared execution/results/recovery and stale-target rejection.
- [x] **P1 — Direct Fetch/Push with options arrows.** Use supported configured single-branch targets; preserve split Pull and modal Commit. No menu-time preparation.
- [x] **P1 — Deliver 0.29.0.** Nine focused scenarios passed across history checks and the corrected network rerun; Dark/Light normal/minimum offscreen review; 26 routine checks in 62.4 seconds, matching ignored preview/package hashes, owning docs and descriptive commit. No interactive launch or live server test.

0.30.1 addresses the identified EOM discard bottleneck; 0.30.0 adds conflict resolution and startup/tab/stash usability. Routine verification remains 26 checks (maximum 30). Large-batch and cold-cache costs remain below.

- [x] **P1 — Direct default Pull.** Main button uses Rebase/auto-stash off and configured upstream; arrow keeps the existing editable modal. Shared real execution/results/reconciliation; no options-time preparation or remembered-choice change.
- [x] **P1 — Deliver 0.28.0.** Four focused checks/offscreen Dark/Light normal/minimum review; 26 routine checks in 51.6 seconds; matching ignored preview/package hashes, updated docs and descriptive commit. No interactive launch or live server test.

- [x] **P1 — Record daily-use priorities.** Prioritize fewer-click Pull, history context actions and in-app conflict resolution; keep LFS locking and coworker distribution later. Do not mark these as implemented.

- [x] **P1 — Diagnose draft storage and preserve both writers.** Twelve repeats of the original reopen flow passed; a controlled publication lock reproduced `CommitDraftUnavailable` with OS error 32. Add bounded cancellable retries for sharing/lock errors, rechecking identity/revision, and allowlisted diagnostic metadata. Four focused checks pass; the old uninstrumented failure's exact cause remains unknown.
- [x] **P1 — Deliver 0.27.1.** Four focused checks; 26 routine checks in 54.0 seconds; zero build warnings/errors, matching ignored preview/package hashes, updated owning docs and descriptive commit. No interactive launch; original failure attribution and physical crash/ACL/disk-full validation remain limits.

- [x] **P1 — Clean workspace and expose remote setup.** Sidebar has no horizontal scroll or fixed width cap; duplicate title/Local Changes and main footer removed. Page range/navigation move into View with keyboard/edge scrolling. Larger caption icons and shorter tooltips; read/refresh fade feedback; one stash base connection preserves real commits. Add remote locally, then explicit Fetch and Set upstream.
- [x] **P1 — Deliver 0.27.0.** Four focused checks and Dark/Light normal/minimum offscreen review; 26 routine checks in 47.4 seconds; matching ignored preview/package hashes, owning docs and descriptive commit. No interactive launch or live network; see roadmap limits.

- [x] **P1 — Compact caption and stable history.** Menus in draggable title strip; Fetch/Pull/Push/Commit order; one row per stash, fixed lanes and broad graph turns. Original metadata and independent history retained; visible pagination/ref jumps agree.
- [x] **P1 — Deliver 0.26.0.** Four focused topology/stash/page/caption checks, offscreen normal/minimum Dark/Light review; 26 routine checks in 41.8 seconds, matching ignored preview, docs and descriptive commit. Native caption behavior and large-repository stash projection timings remain validation limits.

- [x] **P1 — Stable operation feedback.** Fixed duration capsule for existing timed commands; accurate completion outcome, brief hold/fade, no horizontal text jump. Read feedback added in 0.27.0. Retain results/errors; respect animation preference/high contrast.
- [x] **P1 — Deliver visual pass (0.25.0).** Consistent small radii/dividers in both themes, four focused checks and normal/minimum offscreen review; 26 routine checks in 32.7 seconds, matching ignored preview, owning docs and descriptive local commit. Live animation/desktop accessibility remains unverified; see roadmap.

- [x] **P1 — Workspace presentation and toolbar.** Background-only author peers, straight/rounded graph, evenly filled tabs, Open in Explorer/Rider/VS Code, Bash and persisted Dark/Light appearance. Four focused checks and normal/minimum offscreen review; no live tool launches.
- [x] **P1 — Deliver 0.24.0.** 26 routine checks in 32.2 seconds, matching ignored preview/package hashes, owning docs and local commit.

- [x] **P1 — Make repository sidebar functional.** Branch switch/tracking/context actions; fetched remote refs; tag/stash history navigation; inline Apply/Pop/confirmed Drop; submodule open/update/sync. Async cached lists after history, retained errors, real disposable workflows and normal/minimum offscreen review.
- [x] **P1 — Deliver 0.23.0.** 26 routine checks in 31.5 seconds; four focused sidebar checks, offscreen input/layout review, matching ignored preview/package verification and owning documentation.

- [x] **P1 — Reduce routine verification.** Explicit small default, count/runtime guards, first-failure stop, per-check timings, optional full suite, updated guidance and verified 0.22.1 preview. No automatic suite growth or repeated unchanged checks. [Policy](AGENTS.md), [commands](README.md#restore-build-and-check).

- [x] **P1 — Initialize local repositories.** Fast options, new/existing nonrepo folders, optional initial branch, preserved files, automatic opening and real progress/errors/cancellation. Nine focused checks pass.
- [x] **P1 — Deliver 0.21.0.** 216 full checks and nine focused checks passed; offscreen review, owning docs, matching ignored preview and local commit. No interactive launch. [Evidence](docs/roadmap.md#current-status).


## Parallel assignments — Task 1, Task 2, Task 3

Confirmed 2026-10-29: these are independent implementation branches, not three releases. Start each from the same planning commit containing this section. **Use a separate worktree for each chat**; switching branches in one shared directory is not isolation. Never reset another checkout or copy uncommitted work automatically. Do not start other tasks or spawn agents from an assigned chat.

Worker rules: no dependency installation, restore, compilation, test execution, app launch, packaging, version/changelog edits, push or merge. Write meaningful tests and perform static review only; clearly report them as unexecuted. Preserve all existing protections and preparation-free options. Commit assigned changes locally. Do not edit this checklist or shared product/architecture/roadmap docs on worker branches; write a concise implementation/verification handoff in the assigned `docs/handoffs/task-N.md` instead (not another checklist). The integrator will update owning docs and TODO once.

### Task 1 — Local branch deletion and upstream tracking

- [x] **P1; branch `codex/task-1-branch-management`.** Deliver real branch management from the existing Branch menu: delete a selected local branch, set its upstream to an existing remote-tracking branch, or unset its upstream. No remote deletion, force deletion, fetch or implicit checkout.
- Open options immediately from cached refs. Submission revalidates identity/refs/config and serializes through the existing common-repository mutation gate. Confirm deletion with exact branch/tip; use Git's normal merged-branch protection. Refuse current branches and branches checked out in any linked worktree. Handle stale targets, missing refs, active sequences, locks and cancellation accurately; refresh history/ref labels/ahead-behind after success or failure.
- Own new `Core/Git/BranchManagement.cs`, `App/ViewModels/ShellBranchManagement.cs`, dedicated branch-management view/model files, and necessary narrow wiring in `ShellActions.cs`, `ShellViewModel.cs`, `MainWindow.xaml`, `UserInteraction.cs`. Reuse existing Core gate helpers; any necessary edit to `GitActions.cs` must be small and preserve other commands. Do not edit commit/draft/history-performance modules.
- Add disposable-fixture checks in `tests/GitGui.FoundationChecks/BranchManagementChecks.cs`: merged/unmerged/current/linked-worktree deletion, tracking set/unset, changed target and failed/cancelled execution. Handoff: `docs/handoffs/task-1.md` with exact test registration instructions and known limits. Acceptance is source-complete workflow plus written checks; it is **not runtime-verified** until integration.

### Task 2 — Crash-safe per-repository commit drafts

- [x] **P2, advanced for independent parallel work; branch `codex/task-2-commit-drafts`.** Persist commit subject/body locally so closing a tab or restarting Shovel does not lose the draft. Restore after the commit modal is visible; never delay opening/history. Successful commit clears only the submitted draft; failed/cancelled commit retains it. Do not persist passwords, output, diffs or authentication data.
- Use a small dedicated Core draft store and App coordination module. Debounce/bound writes, use atomic replacement, handle corrupt/newer data, permissions/disk failure, shutdown and late completion. Key drafts by repository/worktree identity so linked worktrees stay separate and replaced repositories cannot silently inherit drafts. Avoid lost updates across profiles; expose a localized recoverable save/load problem without blocking ordinary Git use. Preserve newer text when a delayed save/delete completes.
- Own new draft storage/coordinator files, narrow wiring in `CommitViewModel.cs`, `ShellChanges.cs`, `LocalChangesState.cs` if present, and dedicated draft tests. Do not change shared settings schema/profile identity, branch actions, main-window menus or history algorithms. Inspect the actual state type before editing; do not invent a parallel draft state.
- Add isolated storage/lifetime tests in `tests/GitGui.FoundationChecks/CommitDraftChecks.cs` covering reopen, successful/failed commit, corrupt/locked storage, identity replacement, linked worktrees and stale writes. Handoff: `docs/handoffs/task-2.md` with storage/retention design, test registration and unexecuted checks. Draft files contain user text: document their local location and deletion behavior; never log their content.

### Task 3 — Representative history/performance harness

- [x] **P1; branch `codex/task-3-history-benchmarks`.** Extend the existing disposable performance harness with reproducible merge-heavy and large-history fixtures, including an opt-in million-commit case. Generate locally with isolated config, no network or user's repositories. Keep expensive cases opt-in, outside the default foundation suite.
- Measure first history page, warm tab switch, refresh, later pages and cancellation under load separately. Include app/owned-child memory sampling and an opt-in five-minute idle CPU sample, with clear measurement boundaries and machine/fixture metadata. Preserve the existing 200-row page size and All Commits scope; validate topology/count expectations. Never label headless measurements as visible desktop startup or input latency.
- Own new files under `tests/GitGui.FoundationChecks/` prefixed `HistoryPerformance`, plus an optional dedicated `scripts/Measure-History.ps1`. Do not alter production code, existing benchmark methods, shared test dispatch or release scripts. Provide exact integration commands/dispatch registration in `docs/handoffs/task-3.md`. Add bounded cancellation, cleanup of only owned temporary fixtures, and useful failure output; do not generate/run the fixtures in this assignment.
- Acceptance: runnable harness source and documented reproducible commands/metrics, statically reviewed; **no fabricated measurements**. Any optimization suggested by the harness remains a later evidence-based change.

### Shared files and integration

Task 1 and Task 2 may append localization entries to `Strings.resx` using disjoint `BranchManage*` and `CommitDraft*` prefixes; do not reformat or reorder existing entries. This is an explicit small merge point. All other shared-file ownership follows the boundaries above. If an unforeseen overlap is required, document it in the handoff and stop that portion rather than editing another task's modules.

- [x] **P1 — Integrate the three branches after review.** User-authorized merges preserve all worker branches; additive resources reconciled, tests/opt-in dispatch registered, early-open draft persistence corrected, docs/version/changelog updated. 263 full checks and package verification passed; original worker handoffs remain historical. [Evidence and limits](docs/roadmap.md#current-status).

## Next priorities

- [ ] **P2 — Remaining LFS performance acceptance.** 0.36.1 reduces the 64 × 4 MiB fixture median to 2.01 s while verifying exact bytes; native checkout itself is about 1.01 s. Verify cold remote hydration, larger/mixed selections and precise EOM desktop timings when authorized. Missing local objects without a remote, partial staging, concurrent bytes/index changes and cancellation have focused coverage. Preserve honest completion; do not bypass native LFS or hooks to reach a timing target. [Evidence](docs/roadmap.md#lfs-discard-refactor-0361-beta).
- [x] **P2 — External merge-tool chooser (0.36.0 Beta).** Select cached configured/common Git tool names in the inline conflict pane; explicit native launch, actual staged-result checking, unchanged Git configuration and stale/failure handling.
- [ ] **P2 — Remaining conflict editor extensions.** Explicit BOM-less/legacy/working-tree encoding support. UTF-8, BOM-marked UTF-16 LE/BE, named tools and whole-side workflows are delivered; executable-path/custom-command configuration is delivered in 0.43.0. Block-level merge controls remain deferred; the owner chose whole-file sides.

- [ ] **P1 — Finish public installer rollout (0.30.0 branch).** Packaging, signed checks/downloads and explicit restart admission are implemented. GlebHmarsky/Shovel is now the public source/release host; shovel-releases is a legacy migration endpoint with signed 0.30.103 and 0.30.104 releases, and anonymous production feed/package verification passed. Back up the permanent key, choose Windows publisher signing, and verify two installed versions on a disposable Windows user/VM. Prove native install/update/repair/uninstall, cross-session admission, retained settings/drafts and framework maintenance-log contents before coworker rollout. [Setup and limits](docs/distribution.md). Online 0.30.103 → 0.30.104 exposed a schema-6 profile compatibility gap. Repair 0.30.105 supports the saved format and preserves layout; 0.30.106 is its online update target. Fourteen updater regressions and both 26-check smoke runs pass; installed Help-menu/restart acceptance remains pending. Isolated offline ShovelLocalTest 0.30.101 → 0.30.102 native installation/update/uninstall and scoped cleanup passed; interactive installed acceptance remains pending; anonymous production transport verification has passed. See docs/local-installer-test.md.


- [ ] **P2 — Remaining signing/input recovery.** Broaden only for a concrete daily-use failure; not a prerequisite that displaces requested functionality. Existing hooks/signing stay enabled.
- [ ] **P1 — Representative latency validation.** With authorization, measure the user's asset repository and desktop; 0.16 targets scan/read dependencies with disposable fixtures, not an absolute timing guarantee for all disks/filters/repos.
- [ ] **P1 — Extend authentication when needed for network rollout.** Tracked GCM/browser, signing and interactive-hook interaction; preserve shared agents/browsers and hooks/host verification. Askpass exists. Prioritize working Git features; auth expansion is not a blanket prerequisite. Live provider/desktop tests require separate authorization.
- [ ] **P1 — Measure representative startup.** With launch authorization: cold/warm visible rows, keyboard/tabs, Windows 10/11, custom Git, profile restart/replacement. No interactive launch currently authorized. This validation is not a blanket blocker for independent features.
- [x] **P1 — Run representative performance fixtures.** 0.39.1 completed merge-heavy, 100k, million and five-minute idle samples. F1e preserved; headless/child metrics and desktop limits documented in [roadmap](docs/roadmap.md#representative-history-validation-0391).
- [ ] **P1 — Validate accessibility/layout.** Mixed DPI, OS high contrast/screen reader, keyboard/focus, +40%/RTL, themes. Offscreen colors do not certify OS accessibility.
- [ ] **P2 — External reconciliation/tools.** Focus/resume/external-client races and tracked tool sessions.
- [ ] **P2 — File-menu extensions.** Optional all-parent file history/extra diff-tool chooser, repository/global ignore editing and LFS migration; current implemented scopes remain visible.
- [ ] **P2 — Filtering when requested.** Ref/search ancestry, default All Commits, retained selection/scroll.

## Backlog

- [ ] **P2 — LFS locks (low priority).** Discuss a separately authorized server-capability/authentication test, then consider owner display and Lock/Unlock. Server support is unknown; no live test or fetch yet.
- [ ] **P2 — Theme polish (low priority).** Extend the [shared pattern](docs/theme.md): popup/focus/hover/mixed-DPI/RTL and OS high-contrast desktop review. Preserve fast views and native control behavior; do not delay Git functionality.
- [ ] **P2 — Diff/editor extensions.** Individual-line staging, broader hunk file types, image previews, chosen encodings, ignored-file inspection and further draft recovery UX; preserve bounded reads and clear unsupported states.

- [ ] **P1 — Broader network authentication/input.** Credential Manager/browser sign-in, custom SSH commands and interactive signing/hooks remain. Clone and local remote add/edit/remove, upstream management, Pull rebase/merge/auto-stash and sequence recovery are delivered. Add explicit longer/shallow/bare/recursive clone options only with supported UI and failure coverage; current bounds are documented.
- [ ] **P2 — Installed distribution acceptance.** 0.30.0 selects Velopack/public GitHub Releases; finish the explicit rollout task above and pass U1–U7/D1 on installed Windows, including key-rotation and uninstall-data-choice requirements.
- [ ] **P2 — Advanced workflows.** Merge/rebase/interactive rebase/cherry-pick/tag management/reflog/force-with-lease and advanced stash variants; disposable destructive fixtures. Reset/Revert and per-file conflict editing are delivered; extensions remain above.
- [ ] **P2 — Extended compatibility.** Additional submodule management (add/remove/deinit/recursive/remote-latest), worktree/LFS/shallow/partial/sparse management, tray and large-repo optimization. Selected submodule open/update/sync is delivered.

## Decisions before rollout

- [ ] **Q2:** coworker Windows editions/builds, architectures, IT/proxy/no-admin constraints. Local SDK prerequisites are present.
- [ ] **Q3/Q4:** release publication/access and certificate/key custody; GlebHmarsky repository URLs are configured. Public GitHub Releases, Velopack and MIT are confirmed; no private-host/VPN requirement.
- [ ] Refresh pinned .NET 10 servicing patch before distribution; reverify.
- [ ] Finalize Windows publisher signing before distribution. Per-user Shovel install identity retains existing Foundation settings and CommitDrafts locations. Shovel name is confirmed.

## Completed

- [x] **0.30.0 / 2026-10-30 (installer branch):** Windows self-contained packaging, signed public-GitHub update checks/downloads, restart admission and publishing scripts; MIT source license. 12 focused checks, 26 final smoke checks, local installer and disposable signing-fixture packaging verified. GitHub owner and installed rollout remain in the explicit task above. [Evidence](docs/roadmap.md#current-status).

- [x] **0.24.0 / 2026-10-29:** background-only author peers, rounded graph lines, equal tabs, Explorer/Rider/VS Code/Bash toolbar and persisted Dark/Light appearance. Four optional focused checks, normal/minimum offscreen review; 26 routine checks in 32.2 seconds and verified preview. Actual desktop handoffs remain unverified. [Evidence](docs/roadmap.md#current-status).

- [x] **0.23.0 / 2026-10-29:** branch/remotes/tags/stash/submodule sidebar with real navigation and actions, saved stash history, selected submodule open/update/sync. Default remains 26 checks; four optional feature checks and normal/minimum offscreen review. Matching preview hashes verified. [Evidence and limitations](docs/roadmap.md#current-status).

- [x] **0.22.1 / 2026-10-29:** default/preview reduced to 26 explicit smoke checks; 30-count cap, 30 seconds/check, 180 seconds total. All 26 passed in 33.3 seconds; three guard probes and package hashes verified. Extended suite retained but not rerun for this delivery. [Evidence](docs/roadmap.md#current-status).

- [x] **0.22.0 / 2026-10-29:** integrated branch deletion/upstream management, persistent per-worktree drafts and opt-in performance harness. 263 full checks; focused branches 13/13, drafts 28/28, harness 6/6; offscreen review and verified preview. Original branches retained. One earlier intermittent draft write remains tracked above. [Evidence and limits](docs/roadmap.md#current-status).

- [x] **0.21.0 / 2026-10-29:** local repository initialization with fast options, optional initial branch, preserved files/partial results and automatic opening. 216 full checks; focused 9/9; normal/minimum offscreen review and package hashes verified. Next: branch deletion/tracking. [Evidence and limits](docs/roadmap.md#current-status).

- [x] **0.20.0 / 2026-10-29:** confirmed selected Discard/Stash, configured External Diff, first-parent file History/Blame, Ignore/LFS rules and binary patch export. 207 checks; warmed menu/history layout 10/23 ms with zero opening-time queries, selected Discard 667 ms on a disposable fixture. Normal/minimum/menu review and package hashes verified. [Evidence and limits](docs/roadmap.md#current-status).

- [x] **0.19.0 / 2026-10-28:** Local Changes right-click menus, real selected/all index actions, asynchronous Windows file handoff and path copying. 192 checks; zero opening-time Git calls, 11 ms warmed offscreen menu layout, normal/minimum review and verified package. [Evidence and limits](docs/roadmap.md#current-status).

- [x] **0.18.0 / 2026-10-28:** stash create/inspect/apply/pop/drop, cached opening and real saved-object diffs; exact-target deletion, retained conflicts/partial results and cancellation. 187 checks; zero opening-time Git calls, normal/minimum offscreen review and verified package. [Evidence and timing limits](docs/roadmap.md#current-status).

- [x] **0.17.0 / 2026-10-28:** real Continue/Skip/Abort recovery, cached controls and reviewed-target confirmation, shared progress/cancellation and preserved conflict state. 176 checks; zero opening-time Git calls, 44 ms warmed offscreen layout; normal/minimum review and verified package. [Evidence](docs/roadmap.md#current-status).

- [x] **0.16.1 / 2026-10-28:** native dark title bars on app-owned windows, system high-contrast defaults and Windows 10 fallback. 157 checks passed on fresh rerun; hidden HWND API acceptance verified, visible paint unverified. Preview checksums/build/tested assemblies match. [Evidence](docs/roadmap.md#current-status).

- [x] **0.16.0 / 2026-10-28:** scoped file-action results/quiet background scanning, Old/New binary extension/size cards and local LFS metadata. Fixture median affected rows: Stage 1.88→0.50 s, Unstage 1.71→0.51 s; user's exact 11.5-second case unverified. 156 checks, normal/minimum offscreen review, 16 preview checksums/nine build files/four tested assemblies matched. [Evidence](docs/roadmap.md#fast-file-actions-and-binary-cards-2026-10-28).

- [x] **0.15.0 / 2026-10-28:** historical files/diffs with selectable merge parents, binary sizes and shared M/+/− markers; narrowed working-copy metadata and exposed phase timings. Fixture median fresh rows: Stage 2.21→1.11 s, Unstage 2.67→1.26 s; user's 10-second case not reproduced. 150 checks, normal/minimum offscreen review, 16 preview checksums/nine build files/four tested assemblies matched. [Evidence](docs/roadmap.md#committed-files-and-local-action-latency-2026-10-28).

- [x] **0.14.0 / 2026-10-28:** complete-block stage/unstage, multi-hunk selection, stale preview/file/index/attribute checks, clear unsupported states and retained drafts. 140 checks, normal/minimum offscreen review, 16 preview checksums/seven matching tested files. [Evidence](docs/roadmap.md#selected-hunks-2026-10-28).

- [x] **0.13.0 / 2026-10-28:** modal commit workspace, colored/numbered diffs/binary sizes, file/all stage/unstage, commit/amend/Shift push, retained drafts and honest partial-success recovery. 129 checks, normal/minimum offscreen review, 16 preview checksums/seven matching tested files. [Evidence](docs/roadmap.md#modal-commit-workspace-2026-10-28).

- [x] **0.12.1 / 2026-10-28:** shared dark palette/styles for workspace and existing dialogs, high-contrast mapping, documented state conventions. 119 checks; normal/minimum offscreen review; 16 preview checksums/seven matching tested files. No desktop launch.

- [x] **F1g / 0.12.0 / 2026-10-28:** read-only Local Changes, separate index/worktree/untracked/conflicted rows, optional ignored paths, immediate cached navigation, stale-result/cancellation handling and virtualization. 117 checks, normal/minimum offscreen review; 16 preview checksums/seven matching tested files.

- [x] **0.11.1 / 2026-10-28:** cached editable/close-only operation dialogs, zero opening-time Git calls, post-submit validation/cancellation/stale-ref errors. 109 checks, headless first/warm measurements and offscreen review; 16 preview checksums/seven matching tested files.

- [x] **F2d / 0.11.0 / 2026-10-28:** dialogs open before async target preparation; default Rebase/alternative Merge/optional tracked auto-stash, sparse/superproject Pull, retained conflicts/recovery guidance. 106 checks, normal/minimum offscreen review, seven-sample timing runs, 16 checksums/seven matching tested files. Headless harness isolated from production startup; intended source/docs committed with preview ignored.
- [x] **F2c / 0.10.0 / 2026-10-27:** options close on submission; shell-owned Git execution/top-center phase/cancel/retained output through reinspection. Real incoming-commit/file Pull, refreshed history/counts, failures/cancellation; 93 checks, normal/minimum offscreen review, 16 preview checksums/seven matching tested files.
- [x] **0.9.3 / 2026-10-27:** ordinary branch/network actions without extra program consent; omitted unfinished toolbar/sidebar/dialog controls; localized unavailability help. 89 checks, normal/minimum offscreen review, 16 preview checksums/seven matching tested files.
- [x] **0.9.2 / 2026-10-27:** compact all ten maintained Markdown files; require working features and normal operation consent; remove routine trust/prototype mandates. Documentation/version delivery; verification recorded in roadmap.
- [x] **F2b / 0.9.1 / 2026-10-26:** Git/OpenSSH askpass, bounded operation pipe/lifetime, masked owned prompts/wait state/cancellation/reconciliation; 87 checks, offscreen review, 16 checksums/seven matching tested files.
- [x] **F2a / 0.8.0 / 2026-10-25:** real branch/network actions and ref badges; 77 checks, offscreen review and verified preview.
- [x] **F1f / 0.7.0 / 2026-10-25:** workspace layout/upstream counts; 65 checks, headless layout/timings.
- [x] **F1e / 0.6.0 / 2026-10-25:** direct contained Git, prompt-free history/schema 4, tab menus/MMB, packed 100k fixture; 61 checks.
- [x] **F1d / 0.5.0 / 2026-10-25:** automatic history/cache/author relation; 58 checks; paired median 3.89 → 2.29 s.
- [x] **0.4.2 / 2026-10-25:** selected icon/source/nine ICO sizes; 49 checks, extracted-icon inspection.
- [x] **0.4.1 / 2026-10-25:** headerless history/accessibility help; 49 checks, offscreen review.
- [x] **F1c / 0.4.0 / 2026-10-24:** All Commits, refs, 200-row pages beyond 2,000, full details/failure retention; 49 checks.
- [x] **0.3.1 / 2026-10-24:** Shovel title/executable/metadata, compatible profile; requested old-preview cleanup; 45 checks.
- [x] **F1b / 0.3.0 / 2026-10-23:** real HEAD history/graph/details/bounded loading; 45 checks.
- [x] **F1a / 0.2.0 / 2026-10-23:** async bounded runner, real metadata/worktree identity/markers, persistent tabs/migration, shutdown drain, versioned previews; 35 checks and limited published-window inspection.
- [x] **F0 / 0.1.0 / 2026-10-23:** confirmed C#/WPF/.NET 10, Git initialization/ignore, MVVM settings/resources/diagnostics/lifecycle; 17 checks and Windows 11 layout/navigation/splitter/save/relaunch smoke test.

Completed milestones retain their historical scope; desktop/live auth, Windows 10, full accessibility/performance and installed updates remain separate verification work.
