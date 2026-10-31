# Changelog

## 0.35.0 — 2026-11-01

- Choose Stable or Beta updates in Help → Shovel updates; persist the choice for automatic launch and manual checks. Stable remains the default.
- Separate signed feeds and GitHub release/prerelease packages, clear pending updates on channel changes and prevent downgrades/equal-version reinstalls.
- Publish this common baseline on both channels. All subsequent task builds default to Beta; Stable requires explicit owner approval after testing.

## 0.34.0 — 2026-11-01

- Check for updates immediately on every installed launch and automatically restart after a verified download, even with periodic checks disabled.
- Drain restored repository reads before restart validation; retain operation/recovery/settings gates and manual retry on blocked updates. Failed/offline checks keep Shovel usable.

## 0.33.0 — 2026-10-31

- Clone repositories from a URL or absolute local path, optionally selecting a branch/tag, with progress, authentication, cancellation and automatic history opening. Preserve native filters/hooks and any partial Git result.
- Edit local fetch/push URLs and confirm remote removal from cached Repository/sidebar menus. Recheck configuration before writing, preserve local files/branches and reconcile partial failures.
- Add focused disposable-Git and headless-theme coverage while retaining the 26-check release smoke suite and signed Shovel update delivery.

## 0.32.1 — 2026-10-31

- Use the public GlebHmarsky/Shovel repository for both source and all future installer releases/update checks, retaining the existing signing key and app identity.
- Require committed master for delivery, push source before publishing, and attach release tags to the exact packaged source commit.
- Publish this migration version once on the previous shovel-releases host so existing clients can switch without reinstalling; retain that host read-only for migration, with no future builds there.


## 0.32.0 — 2026-10-31

- Merge Windows installation and signed GitHub updates into master while retaining the newer conflict editor, retained comparisons, layout persistence, startup cache, activity log and LFS discard fixes.
- Make every delivered build a published update: Build-Release verifies, packages, signs, uploads through the existing GitHub login and checks an anonymous download. Developer compile checks remain local.
- Preserve schema-6 settings and the explicit settings-blocked recovery message. Published versions are immutable and must increase; source remains private and download hosting remains public.

## 0.31.0 — 2026-10-30

- Retain the previous file/commit comparison during loading; preserve unchanged row identities on refresh and open Commit changes when selecting history or stashes.
- Resolve conflicts inside the Commit diff pane with numbered whole-file sources, initially checked side selectors, editable result and real Resolve/stage. Keep normal Git LFS, deletion and rebase behavior.
- Persist Commit dimensions and file/status/message/conflict splits plus history-file width. Darken inactive repository tabs, quiet toolbar arrows and hide zero ahead/behind badges.
- Open a bounded, session-only All/User/Background activity log from the central repository button, with operation outcomes, timings and available Git output.
- Verify retained views, settings migration, inline staging and activity filters with focused checks; retain the 26-check routine suite. Native focus/DPI/accessibility and crash-time layout persistence remain validation limits.

## 0.30.1 — 2026-10-30

- Use Git's batched `checkout-index` for unstaged discard, matching Fork's observed command. Keep LFS/smudge conversion and index hooks; avoid invoking branch-oriented post-checkout automation for file discard. Hook files and branch/staged-discard behavior remain unchanged.
- Reproduce the EOM delay: 32.7 seconds inside the old restore command; the new full pipeline measured 1.3–2.0 seconds for one asset and 1.4–2.1 seconds for two. Verify exact hydrated bytes, preserved stash/hooks, partial staging, lock/filter failures and responsive cancellation. These warm local measurements are not a universal one-second guarantee.

## 0.30.0 — 2026-10-30

- Resolve individual conflicts with named current/incoming sources, native side checkout and staging, or a bounded in-app UTF-8 merge editor. Preserve rebase side semantics, binary/LFS assets, deletion choices, stale edits and failed-tool results. Show an orange merge/rebase banner with Resolve, Continue and Abort.
- Restore the last selected repository's saved history view while fresh data loads; mark it as last-known and keep mutations dependent on current data. Add Apply/Pop/Drop to stash history menus, keep the tab strip themed under the Commit modal, and drag tabs to reorder their saved positions.
- Overlap discard validation and collapse HEAD metadata queries. Correct false modified rows for hydrated standard LFS assets using local pointer/hash comparison, without running filters during background reads. Add isolated single/batch discard profiling; the reported 30-second spike and Fork-equivalent latency remain unverified.
- Add optional conflict, usability and LFS checks; retain the 26-check routine budget. Document cache storage, editor limits, measured performance and remaining desktop validation.

## 0.30.106 — 2026-10-30

- Online update target for the repaired 0.30.105 installer. Includes the same schema-6 profile compatibility and specific blocked-settings guidance; no additional behavior changes.
- To retry from the blocked 0.30.103/0.30.104 build, close Shovel, install 0.30.105 once, then use Help > Shovel updates to download and explicitly restart into this version.

## 0.30.105 — 2026-10-30

- Fix blocked update restarts when a profile was saved by a schema-6 Shovel build. Support its settings structure, preserve commit/history layout fields during window capture, and migrate older schemas without deleting the profile.
- Explain read-only settings as the reason for a blocked restart and provide installer recovery guidance. Unknown future formats remain protected from writes.
- Users blocked on 0.30.103/0.30.104 must close Shovel and run this installer once; no settings deletion or downgrade is needed.

## 0.30.104 — 2026-10-30

- First online update rehearsal for the GitHub-connected Shovel 0.30.103 installer. Check and download through Help, then explicitly restart to install.
- Uses the same application identity and permanent signing key as 0.30.103; retains settings and drafts. No additional Git workflow changes.
- The offline Shovel Local Test 0.30.102 fixture must first be replaced by the regular Shovel installer; it does not contact GitHub.

## 0.30.103 — 2026-10-30

- Provide the GitHub-connected Shovel installer for the first online update rehearsal. Uses the permanent release verification key and GlebHmarsky/shovel-releases stable feed.
- This is the regular Shovel application, separate from the offline Shovel Local Test 0.30.102 fixture. Install this version once before testing an online update.
- Windows publisher signing is not yet configured.

## 0.30.1 (installer branch) — 2026-10-30

- Configure GlebHmarsky/Shovel as the source repository and GlebHmarsky/shovel-releases as the update host, with a permanent public verification key. The private signing key stays outside the repository.
- Compare public/private keys by cryptographic identity so Windows checkout line-ending conversion does not break packaging.
- Default release packaging to the configured repository and public key. Repositories remain private; unattended update access requires making shovel-releases public. No shared GitHub token is embedded.

## 0.30.0 (installer branch) — 2026-10-30

- Add per-user self-contained Windows installer packaging with pinned Velopack, signed GitHub release feeds, automatic check/download, manual retry/cancel and an explicit idle-gated Restart and update action. Preserve existing settings/tabs/drafts.
- Add local release signing/publishing scripts, stable/test channels and GitHub setup instructions. Hosting remains unconfigured until the owner supplies their account; native installed update/repair/uninstall still need disposable Windows validation.
- Publish source under the MIT license with third-party notices.

## 0.29.0 — 2026-10-29

- Add history-row context actions for detached Checkout, confirmed Soft/Mixed/Hard branch Reset, full SHA copying and Revert. Merge reverts explicitly choose a mainline parent; conflicts use the existing recovery controls.
- Keep menu construction local and immediate; freeze clicked targets, reject stale snapshots, retain Git dirty-file protections and reconcile real outcomes through shared progress/cancellation.
- Give Fetch and Push direct main buttons plus preparation-free options arrows, alongside existing split Pull. Resolve supported default targets from local Git configuration; Commit continues to open its workspace.

## 0.28.0 — 2026-10-29

- Make the main Pull button execute Rebase against the configured upstream with auto-stash off. Add an adjacent arrow opening the existing Pull settings dialog; alternate targets, Merge and auto-stash still execute through that modal.
- Reuse progress, cancellation, authentication, retained output and history reconciliation. Explain missing upstreams and validate the server-side tracking target before direct Pull; no guessed remote or silently remembered modal choices.
- Record confirmed future history checkout in detached HEAD and conflict Resolve applying/staging the chosen side. Those features remain pending.

## 0.27.2 — 2026-10-29

- Record personal Fork-replacement priorities: fewer-click Pull, history Checkout/Reset/Copy SHA/Revert, and named-side conflict resolution with an in-app text merge editor. These are planned features; runtime behavior is unchanged.
- Defer coworker distribution and LFS locking experiments; retain open interaction choices rather than invent defaults.

## 0.27.1 — 2026-10-29

- Recover from temporary commit-draft sharing/lock failures with short, cancellable background retries. Recheck worktree identity and draft revision on each attempt; preserve another profile's newer save and this window's recovery copy.
- Record only draft operation, storage stage and numeric OS error in bounded diagnostics. Persistent locks and other storage failures retain the existing recoverable error; no draft text, repository paths or arbitrary exception messages are logged.
- Add four optional focused checks for replacement, lock recovery, cancellation, competing saves and diagnostic privacy; keep the routine suite at 26 checks. The earlier uninstrumented failure's exact cause remains unknown.

## 0.27.0 — 2026-10-29

- Remove the main footer, history page-control band and duplicate sidebar repository/Local Changes entries. Let the sidebar widen without a fixed cap, trim labels without horizontal scrolling, and enlarge caption icons. Keep page navigation/range in View, add keyboard and scroll-edge paging, and shorten tooltips.
- Show timed history reads/refreshes with a brief fade-in and outcome-colored completion fade. Preserve command feedback through reconciliation, failed-read rows and visible diagnostics.
- Add preparation-free local remote setup and expose upstream set/unset under Repository. Save configuration without fetching; use explicit Fetch for remote branches.
- Distinguish stash nodes and consistently connect them to their true base, preserving real stash-only ancestors and independently reachable internal objects.

## 0.26.0 — 2026-10-29

- Move Commit after Push and combine the main menus with a draggable title strip and working window controls.
- Show one entry per saved stash, folding private index/untracked commits while preserving original details and independently reachable history. Keep full visible pages and correct ref navigation.
- Keep merged branches in distinct straight lanes until their shared ancestor, with broader rounded turns and consistent continuation across pages.

## 0.25.0 — 2026-10-29

- Give timed operations a fixed elapsed-time capsule at top center, with distinct success, conflict, failure and cancellation states. Hold completion briefly then fade; retain results and errors, avoid text movement and honor Windows animation/high-contrast preferences.
- Refine shared button, tab, menu and repository-context borders with consistent small corner radii in Dark and Light themes.
- Require descriptive change-focused commit titles; retain the small routine verification suite and optional focused visual checks.

## 0.24.0 — 2026-10-29

- Highlight same-author history rows with a muted yellow background and unchanged text; retain normal selection and high-contrast behavior. Draw compact straight graph lanes with rounded joins and distinct merge nodes.
- Spread repository tabs evenly across the full strip. Restore working Open in (Explorer/Rider/VS Code), Git Bash Console and Appearance toolbar actions; omit Work.
- Add live Dark/Light palettes and remembered appearance/tool executable preferences. Rider prefers root-level Unreal projects; executable/project discovery happens only after explicit selection, never while opening menus.

## 0.23.0 — 2026-10-29

- Add expandable sidebar branches with tracking counts, fetched remote branches, tags, stashes and submodules. Locate refs/stashes in paged history and use branch/stash context actions without opening their old list dialog.
- Include saved stash commits and labels in All Commits. Preserve useful history during bounded asynchronous navigation and list refresh.
- Open submodules in repository tabs, initialize/update the selected recorded commit, and sync its URL, with shared progress/authentication/cancellation and normal Git checkout protections. No force or recursive update.

## 0.22.1 — 2026-10-29

- Replace default full verification with 26 smoke checks, capped at 30 checks and three minutes overall (30 seconds per check); stop on the first failure and report individual timings.
- Keep the complete regression suite available through `--full` and existing focused commands. Preview packaging runs only the smoke suite; expensive benchmarks remain opt-in.

## 0.22.0 — 2026-10-29

- Merge local branch deletion and upstream set/unset into the Branch menu, with cached options, exact-target confirmation and normal Git merged/worktree protections.
- Persist per-worktree commit drafts locally with atomic revisioned writes, delayed restoration, conflict recovery copies and exact-submission clearing after successful commit.
- Integrate opt-in merge-heavy/large/million-commit performance fixtures, cancellation and process resource sampling. Keep expensive benchmarks outside normal checks.

## 0.21.0 — 2026-10-29

- Add File / Repository → New repository with destination and optional initial branch. Open options without Git/filesystem preparation; Create closes options and runs real initialization with shell progress, output and cancellation.
- Preserve populated folders and Git's configured defaults/templates without adding files, staging, commits or remotes. Open existing worktree roots without reinitializing; explain invalid or unsupported destinations.
- Automatically select/open a usable result, including one observed after cancellation. Preserve partial files and previous tabs on failure; coordinate same-destination commands and retain normal ownership checks.

## 0.20.0 — 2026-10-29

- Extend Local Changes menus with confirmed selected-file Discard and Stash, configured External Diff, file History/Blame, Ignore paths/extensions, LFS tracking rules and binary-capable Save as Patch. Existing Open/Explorer, stage/all and copy actions remain.
- Show exact discard targets and staged/unstaged/untracked consequences before execution; preserve unrelated work and recheck stale selections. Selected stashes exclude unrelated index changes, with explicit partial-success reporting.
- Keep menus/confirmation and file-history windows preparation-free. Read pinned file history incrementally across renames, retain rows on failed refresh, reject cancelled results and reuse real committed-file diffs. Continue targeted post-action reconciliation and bounded background work.

## 0.19.0 — 2026-10-28

- Add right-click menus to staged and unstaged Local Changes rows: selected/all Stage or Unstage, Open, Show in File Explorer, Copy path and Copy full path. Preserve a containing multiselection and freeze menu targets; refreshed snapshots disable stale index actions. Menus do no Git or filesystem preparation, and a newly selected diff waits until the menu closes.
- Add real file-grid/menu shortcuts and visible shortcut labels in shared dark menus. Explicit menu actions ignore the toolbar's Shift-to-all modifier; Enter inside a popup keeps normal highlighted-item behavior.
- Hand explicit working-file requests to Windows away from the UI thread, with bounded admission/wait, localized failures and missing-file folder fallback. Never launch external applications or modify the clipboard merely by opening a menu.

## 0.18.0 — 2026-10-28

- Add Stashes in the sidebar and Repository menu: create with optional untracked files, inspect saved tracked/staged/unstaged/untracked files and text/binary diffs, apply, pop and confirmed drop. Optional staged-state restoration controls Apply/Pop explicitly.
- Open the owned modal from cached entries/draft without Git preparation; read afterward asynchronously, preserve selection by hash and retain rows on refresh failure. Share top-center progress/cancellation and retained output; refresh history/counts after mutations.
- Apply immutable stash objects once, retain conflicted/partially restored entries, and distinguish successful application from uncertain removal. Revalidate reviewed stash selectors against the observed list, reject shifted targets, confirm exact Drop scope and preserve Git protections without automatic retry/cleanup.

## 0.17.0 — 2026-10-28

- Recover interrupted merges, rebases, cherry-picks and reverts through Continue, Skip and Abort in the sidebar and Repository menu. Reopen rediscovers state; no opening-time preparation or remote queries. Resolve/stage files through Local Changes, then continue with Git's existing message.
- Confirm actual Abort/Skip targets and consequences; reject unresolved Continue and changed operation state. Reuse shared mutation coordination, top-center progress/cancellation and retained output. Reinspect after failure/cancellation; preserve subsequent conflicts and auto-stash recovery without automatic cleanup or rollback.

## 0.16.1 — 2026-10-28

- Match native title bars to the dark toolbar on the main, commit, branch/network and authentication windows. Preserve native caption controls; restore system defaults in high contrast. Windows 11 supports exact palette colors; Windows 10 uses a best-effort native dark request.

## 0.16.0 — 2026-10-28

- Show old/new binary file cards with extension, readable size, exact bytes and LFS labels in Local Changes and historical Changes. Read LFS pointer sizes locally without downloading assets or invoking filters.
- Return file Stage/Unstage results from path-scoped status, then coalesce a full refresh in the background. Cancel obsolete reads without waiting for them before a write; retain generation checks, index/config/HEAD validation, real Git results and failures. Unstage uses index-only, literal path-limited reset with no worktree refresh.
- Remove bottom activity bars and the commit window's ticking footer. Keep cancellation, static results/errors and diagnostic phase timings; retain useful content during background refresh and selection across staged/unstaged moves.

## 0.15.0 — 2026-10-28

- Add a Changes tab for the selected history commit: real changed files, numbered colored diffs, rename paths and binary size changes. Compare merge commits against a selectable parent; initial commits compare with an empty tree. Reads stay bounded, cancellable and independent of working files.
- Share amber M, green + and red − file markers between history changes and Local Changes, with text/accessibility meanings and system high-contrast colors.
- Remove full-history ref enumeration from working-copy validation and refresh. Run independent reads within the existing two-process bound; retain index/HEAD/config/identity checks and reconcile real Git outcomes. Display read-drain, validation, Git-command and refresh timings in the commit window output section.

## 0.14.0 — 2026-10-28

- Stage or unstage selected complete change blocks from the commit modal's diff. Ctrl/Shift selects multiple blocks; one context-aware button shows the action and count, with accessible block descriptions.
- Apply only the selected hunks to the index, preserving working files and unrelated staged edits. Revalidate the exact preview, file content, index, attributes, configuration and repository state; report failures/cancellation and refresh without a whole-file fallback.
- Keep drafts and selected file after hunk actions. Explain unavailable actions for binary/filter-managed content, additions/deletions/renames and display-only comparison options. Ordinary tracked UTF-8 text modifications support CRLF normalization and missing final newlines; arbitrary line selection remains deferred.

## 0.13.0 — 2026-10-28

- Open Local Changes as a resizable modal commit workspace over retained history. Add a Commit count after the selected tab's history is visible; opening binds available rows before asynchronous inspection, without polling other repositories.
- Show unstaged/untracked/conflicted and staged lists beside a virtualized, numbered, colored unified diff. Include whitespace comparison, whole-file context within limits, hunk navigation and binary before/after byte sizes.
- Implement selected-file stage/unstage and commit/amend. Hold Shift on the same buttons for Stage all, Unstage all and Commit and Push to the configured upstream. Preserve hooks, confirm amend's actual target, retain drafts on failure and report a successful commit separately from a failed push.
- Revalidate identity/HEAD/config/index before writes, serialize related operations, drain cancellation and reinspect results. Keep bounded session drafts/output, stale rows, clear limits and shared dark/high-contrast styles. Hunk staging, image previews and an interactive conflict editor remain deferred.

## 0.12.1 — 2026-10-28

- Apply a restrained dark client-area theme to menus/toolbar, repository tabs, sidebar, history/Local Changes, lower details tabs and existing operation/authentication dialogs. Distinct charcoal surfaces and orange lower-tab accents follow the supplied reference without copying assets.
- Centralize semantic colors and hover/pressed/disabled/selection/focus styles; preserve native control input, history author highlighting and virtualization. Map colors to Windows high contrast and document the project-wide pattern.
- Keep further theme polish low-priority behind file diffs/staging. Native OS title bars/pickers/message boxes remain system-owned; light-mode selection is deferred.

## 0.12.0 — 2026-10-28

- Add read-only Local Changes with real staged, unstaged, untracked and conflicted entries; ignored paths are opt-in. Preserve rename origins and show both index/worktree changes for partially staged files.
- Switch views immediately; scan asynchronously with cancellation, cached per-tab rows/selection, explicit stale/errors/limits, virtualization and no polling or startup preloading. Preserve history while navigating.
- Suppress configured clean/process filters and fsmonitor during status reads without changing Git configuration or index bytes. Submodule status is pointer-only; content diffs and staging remain next.


## 0.11.1 — 2026-10-28

- Pull/Fetch/Push/branch dialogs use cached local targets immediately; removed opening-time Git preparation and loading/error shells. Fresh validation runs after submission with main-window progress and retained errors.
- All operation options are fixed-size with Close only in the title bar; content remains scrollable. Pull keeps default Rebase, optional Merge and auto-stash.
- Cache configured remote names during local inspection, including unfetched remotes. No opening-time network checks; Fetch discovers new remote branches. Document prompt UI behavior as a project requirement.


Historical entries describe their release, not current policy. From 0.9.2, working features/normal operation consent supersede older prototype/trust requirements.

## 0.11.0 — 2026-10-28

- Open operation dialogs before Git preparation; show loading/errors in place, cancel dismissed preparation and reject late results. Reuse inspected refs and overlap bounded URL reads; add repeatable dialog/preparation timings.
- Pull defaults to real rebase with a merge alternative and optional tracked auto-stash. Integrate divergent branches; permit unrelated untracked files and protect ignored collisions. Explicit strategy/stash choices override conflicting Git defaults without changing configuration.
- Allow Pull in existing sparse worktrees and superprojects, updating submodule pointers without touching submodule worktrees. Branch-switch restrictions remain separate.
- Preserve conflicts and Git recovery stashes, including auto-stash conflicts despite zero exit; show recovery instructions and top-center resolution state. Keep hooks, output, cancellation/reinspection and existing operation admission.
- Cover disposable local remotes, divergent file changes, staged/unstaged restoration, conflicts/reopen/abort, dialog preparation and offscreen layouts. Final checks suppress production startup; desktop interaction and live accounts remain unverified.
- Isolate the headless WPF harness from production startup/profile composition by loading shared resources in a test-only Application.

## 0.10.0 — 2026-10-27

- Submitting Pull/Fetch/Push/branch options closes the modal and runs the operation under main-window ownership. Top-center phase, elapsed time, progress and Cancel remain visible through completion/reinspection.
- Retain success/error/cancellation and bounded Git output in an Operation details tab; distinguish refresh failure, prevent duplicate/concurrent actions and keep results scoped to their repository. Authentication prompts use the main window after options close.
- Verify real incoming-commit/file Pull behavior, refreshed history/counts, dirty/unavailable-remote errors, cancellation and offscreen layouts. Prioritize Local Changes/status before broader authentication expansion; Pull remains clean-worktree fast-forward only.

## 0.9.3 — 2026-10-27

- Normal Branch/Fetch/Pull/Push actions execute without a separate configured-program consent checkbox or execution flag. Required authentication, hooks, Git protections and cancellation/reinspection remain.
- Omit unfinished Stash/external-tool/sidebar entry points and rebase/autostash/force options. Keep the functional menus, branch controls, history and details; explain unavailable action targets/states with localized accessible help.
- Exercise all five ordinary action dialogs against disposable repositories/local bare remotes, alongside failure/cancellation/authentication regressions and normal/minimum offscreen layouts. No interactive launch or live provider testing.

## 0.9.2 — 2026-10-27

- Compact all ten maintained Markdown files; consolidate UX, acceptance, mechanisms, evidence and tasks while retaining requirement IDs, release history and research references.
- Require functional features end to end; remove UI-first/placeholder-retention mandates and phase-order blockers for newly requested work.
- Prohibit routine repo-trust/first-use/program-consent layers and extra safety measures without concrete need. Retain normal Git protections, authentication and destructive-action confirmations.
- Track remaining approval/prototype code changes in TODO. Runtime behavior unchanged; versioned local preview only.

## 0.9.1 — 2026-10-26

- Owned Git/OpenSSH askpass for explicit Fetch/Pull/Push: masked replies/confirmation, known waiting state, cancel/reinspection.
- Packaged helper: operation-only current-user pipe/nonce/server checks, bounded frames/requests/deadlines, cleared reply buffers; no history helper.
- Cached GCM/agents/SSH host verification retained; browser/signing/custom SSH/interactive hooks pending, no live-account/desktop certification. Real disposable credential/helper tests and offscreen prompts; no dependency/installer/updater.

## 0.8.0 — 2026-10-25

- Real Branch create/switch and Fetch/FF Pull/normal Push, optional fetch-all/tags/upstream; original local/remote/tag/current/same-tip-upstream badges.
- Added then-required program approval, identity/ref/config revalidation, shared-worktree lock, modal/shutdown admission, bounded output/cancel/reinspection. Automatic history remained prompt-free.
- Preserved hooks/cached auth; interactive/custom SSH and rebase/autostash/force pending. Local bare-remote/failure/cancel/offscreen checks, no desktop/live auth/distribution.

## 0.7.0 — 2026-10-25

- History directly below tabs, compact footer; removed selector, fixed local/remote/tag/current-HEAD scope, ignored older narrower settings.
- Restored menus/toolbar/sidebar/center repo-branch-timer; unfinished controls were disabled prototypes (historical policy).
- Real local-upstream behind/ahead after rows, dash for unavailable; no network/action implementation then. Disposable divergence/cancel/offscreen checks; cache/author/read behavior retained.

## 0.6.0 — 2026-10-25

- Removed local-history consent/management; opened/restored reads automatic, schema-4 old fingerprints become replacement stamps.
- Direct native Git in atomic Job Object; no per-command .NET helper, prefer native Git, combine HEAD/refs/reuse observations. Preserved limits/cancel/ownership/post-validation.
- Tab menu refresh/move/close/close others, MMB close; active cancels/inactive preserves read. Native/process/tab checks, packed 100k benchmark; no interactive/network/distribution work.

## 0.5.0 — 2026-10-25

- Selected approved startup repo/cache restoration; bounded page/scope/selection/scroll, async refresh retaining rows.
- Then-current remembered/revocable identity approval/migration/replacement checks (superseded 0.6.0).
- Combined queries/parallel HEAD pairs/reused refs/source-generated protocol; measured timing/diagnostics.
- Secondary actions in Repository; removed prototypes then; yellow exact-author peers/high contrast/accessibility. Read-only, no desktop/network/mutations/updater.

## 0.4.2 — 2026-10-25

- Selected front-facing silver shovel/blue circle as EXE/window icon; source PNG and multi-size transparent ICO.

## 0.4.1 — 2026-10-25

- Headerless history per user screenshot, aligned columns/localized help/row descriptions; no header drag, description adapts. Scope/paging/status retained, Git unchanged.

## 0.4.0 — 2026-10-24

- Default local/remote/tag All Commits plus then-separate current-HEAD scope/labels.
- Replaced 2,000 cap with 200-row older/newer/range/limits, cross-page graph cursors; minimum layout reserves rows, resizable details.
- Complete parsed details; failure/cancel retains page, ref/boundary changes require refresh. Real multi-branch/merge/nested-tag/packed/shallow/detached/unborn/2,005 fixtures headlessly; no desktop/mutations/network.

## 0.3.1 — 2026-10-24

- Shovel localized title/EXE/product; compatible GitGui profile/mutex/internal names.
- Requested old Build cleanup/preview rebuild/docs; Git scope unchanged.

## 0.3.0 — 2026-10-23

- Real virtualized HEAD graph/refs-free description-author-date-hash table, merge second ring; full selected message/identities/dates/hash/parents.
- 200 initial rows, bounded older expansion to then-2,000 cap/pinned tip, explicit shallow/empty/errors/manual refresh.
- Real linear/merge/shallow/signature-suppression/stale/binding/virtualization checks; no desktop. All-ref/filter/status/files/diffs/mutations pending then.

## 0.2.0 — 2026-10-23

- Then-trusted local metadata: branch/HEAD/worktree/common identity/markers/manual refresh.
- Persistent tab order/selection (no restored trust), reorder/close; schema 1 → 2/backup.
- Async streams/caps/deadlines/cancel/owned jobs through waiting helper; isolated fixtures/ignored versioned preview. History/status/diffs/mutations/auth/distribution pending then.

## 0.1.0 — 2026-10-23

- C#/WPF/.NET 10 MVVM shell, English resources, settings/diagnostics/lifetime.
- 17 checks and actual Windows 11 GUI smoke.
