# Product

Updated 2026-10-31. [Requirements](requirements.md) owns acceptance; [architecture](architecture.md) owns mechanisms; [roadmap](roadmap.md) owns delivery evidence.

## Clone and remote management (0.33.0)

**File / Repository → Clone repository** opens immediately with source URL or absolute local path, destination and optional branch/tag. Submit clones into a new or empty directory with progress, Cancel and the existing authentication prompts, then opens the resulting repository and history. Normal filters, LFS and checkout hooks remain enabled. A cancelled/failed clone reports its result and leaves any native Git partial output in place; Shovel opens a valid remaining repository without replaying the clone or deleting files.

**Repository → Manage remotes**, also available from the sidebar remote menus, uses cached local configuration. Save updates the fetch URL and optional push URL; clearing the latter restores Git's fetch-URL fallback. Remove confirms the remote/repository target, then removes local remote configuration and tracking refs. Neither action contacts the server or changes local branch tips/working files. Execution rechecks repository identity and configuration and refreshes after failures as well as success. Multiple/inherited/credential-bearing URLs cannot be edited in this form; unsupported cases remain visible with a reason.

Clone is a normal full checkout with default `origin`; recursive submodule, shallow/bare controls and custom SSH-command interaction remain outside this increment. [Architecture](architecture.md#clone-and-remote-management-0330) owns execution bounds; live authentication and installed desktop acceptance remain separate from headless evidence.

## Daily-use interactions (0.31.0)

Selecting a file or commit retains the last completed, labelled comparison until the new one is ready. Loading/error status explains the retained view; a completed empty result clears it. Stale visible hunks cannot be staged. Unchanged refreshes preserve row identity and native focus instead of rebuilding the selected rows. Clicking a history commit or stash opens the bottom **Commit** changes tab; metadata remains under **Details**.

The central repository button opens a session activity panel with **All / User / Background**, retained selection, operation outcome/duration and available Git output. It performs no preparation. Entries are bounded and are not persisted. Inactive repository tabs use the dark strip, the selected tab stays gray, and adjacent toolbar arrows use the same quiet surface. Nonzero ahead/behind counts appear beside the icons; zero/unknown counts are hidden.

Commit window dimensions and file-list, staged/unstaged, message and conflict-result splits survive reopening and clean exit. The main history/file splitter is saved too; restored sizes are clamped to the available workspace. Layout changes are not crash-persistent.

Conflict resolution now occupies the existing Commit diff pane. Both whole-file sides start checked; unchecking one previews the remaining source. **Resolve and stage** performs the native side operation and stages the file. Keeping both checked uses the editable result, initially the working file including markers; there is no automatic concatenation or block-level selection. Checkbox changes never mutate Git. This supersedes the separate editor described in the historical 0.30.0 workflow below.

## Purpose

**Shovel** is a lightweight Windows 10/11 Git client for about 30 initial coworkers using development repositories, including C++/TypeScript. Aim for Fork's restrained layout and Git Extensions' practical capabilities, with original assets. AI maintains the code; the shipped app needs no AI service. English first, localization-ready; signed-feed installers and in-place updates are implemented, with broader installed acceptance still pending.

**Confirmed priorities, 2026-10-29:** first replace Fork for the owner's daily work and personal testing; broader coworker distribution comes later. Motivations are inspectable source, control over workflow/visual details, fewer routine clicks and evaluating AI-maintained software. These motivations are not a security finding about Fork or a security guarantee for Shovel. Public MIT source and all new releases share GlebHmarsky/Shovel (2026-10-31); the previous separate release host is an archived migration endpoint. [Distribution](distribution.md) owns publishing and recovery.

<a id="requested-next-workflows-not-implemented"></a>
## Requested workflow direction

- **Conflict resolution:** delivered in 0.30.0; see [the workflow and current limits](#conflict-resolution-and-faster-reopening-0300). The supplied Screenshot_19.png informed the requested named-side layout; no broader visual-parity claim.
- **LFS locking:** low priority, desired for a later experiment. Server capability/authentication and actual lock workflows are unverified. No live server test or automatic fetch is authorized by this planning discussion.

The general diff-view preference question was left unanswered; no side-by-side default, image-preview priority or individual-line staging decision is inferred. [TODO](../TODO.md#next-priorities) owns sequencing.

**Confirmed 2026-10-27:** deliver usable functionality with its UI. Every requested button/feature must perform its Git/tool action and expose real results, progress and errors. No new placeholder/no-op controls or UI-only completion claims. Existing disabled prototypes are debt to implement or omit as their area is revised, not a permanent design. Temporary disabling is for actual unavailable state/dependencies.

The work environment is already trusted. No repository-trust modal on open, restore, first operation or ordinary dialog opening; no repeated consent checkbox for configured Git programs. Selecting the normal action authorizes it. **Do not introduce extra safety measures or approval layers without a concrete requirement.** Retain normal Git data-loss protections, destructive-action confirmation and required authentication/SSH host verification. This replaces older prototype-retention and routine trust-gate requirements; runtime gaps are tracked in TODO.

## Conflict resolution and faster reopening (0.30.0)

Select a conflicted file in Local Changes. Since 0.31.0 its inline diff pane reads actual index stages after display. Current/incoming cards show available branch/ref names and commit IDs; during rebase they are explicitly the rebased result and replayed commit. Stash/patch conflicts with no recorded source retain honest generic labels. Checkboxes only select: **Resolve and stage** applies one chosen side or saves the editable merged result when both are selected, then stages that path. An absent side means deletion. Resolving a file does not continue a sequence automatically.

Ordinary UTF-8 text up to 512 KiB, 20,000 lines and 16,384 characters per line supports numbered side inspection, whole-file preview and direct result editing. Remaining conflict markers are rejected without losing edits. Reload retains typed result text. Binary/LFS/filtered/other-encoding files use native side checkout or the configured external merge tool; no pointer is saved as an asset. The external button uses Git's merge.guitool/merge.tool; an in-app tool chooser remains deferred. Errors, duration and cancellation remain visible; failed staging keeps the saved result, while stale index/config/attributes/file data require reload. Closing unsaved editor text confirms that file. No automatic rollback or backup cleanup.

During a merge/rebase, an orange banner above history offers **Resolve** (Local Changes), **Continue** and **Abort**. Abort uses the active Git operation and its existing target-specific destructive confirmation. Stash conflicts use per-file resolution without inventing a Continue/Abort sequence. History stash rows have their own **Apply / Pop / Drop** menu; Drop confirms the saved target, and submission validates the current selector/hash.

On clean shutdown, Shovel saves the selected repository's last verified history page, selection and scroll. On restart it can show this explicitly labelled last-known view while fresh data loads. It never restores mutation readiness or replays commands. Missing/invalid/replaced-repository cache data is ignored. Tab dragging moves individual tabs left/right and persists their order; the disabled strip retains the selected theme while Commit is open.

Since **0.30.1**, unstaged discard uses Git's file checkout, matching Fork: native LFS/smudge conversion and index hooks remain enabled, while post-checkout branch automation does not run. Hook files, branch operations and staged-discard behavior are unchanged. Exact selected scope, partial staging, progress, cancellation and error reconciliation remain. Standard LFS v1 assets are compared locally with their stored SHA-256 pointer so restored assets leave Local Changes; custom filters/extension pointers remain conservatively visible. [Measurements and remaining limits](roadmap.md#lfs-discard-0301).

## History actions and network split buttons (0.29.0)

Right-click a history row (or use Menu / Shift+F10) for **Checkout (Detached HEAD)**, **Reset branch to here → Soft / Mixed / Hard**, **Copy SHA** and **Revert commit**. The menu captures that row's full object ID and cached repository state; opening it performs no Git or filesystem preparation. Reopen after a tab/page/refresh change. Copy includes the full hash. Checkout enters detached HEAD without creating a branch or forcing over dirty files; the repository panel labels that state.

Reset requires an attached branch. Confirmation names the repository, branch, current tip and target, explaining the selected mode: Soft preserves index/worktree; Mixed resets the index but preserves working files; Hard replaces index/tracked working content and can delete obstructing untracked/ignored files. There is no automatic backup or rollback. The previous tip/output and Git reflog can help recover commits, not all discarded edits. Submodule contents are not recursively reset.

Revert creates an inverse commit using Git's generated message. For a merge, choose the explicit numbered parent/hash; Shovel never guesses a mainline. Existing dirty/index protections and configured hooks/signing remain. Failures retain output/history; a conflicted Revert exposes the existing Continue/Skip/Abort workflow. New mutations are unavailable during an active sequence; Reset is not a substitute for Abort.

**Fetch, Pull and Push** now have a direct main action and an adjacent arrow for their existing settings modal. Commit always opens its workspace. Fetch uses the current branch's configured remote, otherwise origin or a sole remote. Push uses Git's selected push remote and supported single-branch `simple` (default), `current`, or `upstream`/`tracking` destination; it can differ from Pull's upstream. Missing/ambiguous targets, `nothing`/`matching`, custom push refspecs, URL-only/dot remotes and automatic upstream creation require explicit options/setup. Main Push sends only the current branch, without force or tags. Options choices affect that attempt only. Pull defaults remain below. Submission owns validation, authentication, execution and top-center progress/cancellation; menu/options opening never queries a server.

## Direct Pull (0.28.0)

Confirmed split-button behavior: **Pull** executes the current branch's configured upstream immediately; the adjacent arrow opens the existing Pull modal for alternate targets/settings and its normal submit flow. Keep the established default: **Rebase, auto-stash off**. Modal choices apply to that submission only; no new remembered settings. No options window or extra confirmation for the default action. Both routes share actual command progress, cancellation, retained errors/output and history reconciliation.

The arrow remains preparation-free. Direct Pull requires an attached branch and an unambiguous cached remote-tracking upstream; otherwise its localized help points to upstream setup or Pull options. Submission validates the actual configured remote/server branch before fetching, so changed/custom tracking mappings are not guessed from a ref suffix. Unsupported mappings can use explicit targets in the modal. Normal Git dirty-worktree protections, hooks and authentication remain. Opening tabs never initiates Pull.

## Current workspace

Implemented through **0.30.1**; ordinary action buttons authorize their configured Git behavior without an extra consent step.

Branch menu: delete local branch, set upstream to an existing remote-tracking branch, or unset upstream. Options use cached refs; submission validates targets. Deletion confirms branch/tip and retains Git's merged and checked-out-worktree protections. No force deletion or remote operation is implied.

Commit drafts now survive closing tabs/restarting. Restore happens after the modal appears; newer typing wins. Successful commit clears only its submitted text, while failed commit retains it. Storage errors remain visible without blocking Git; conflicting copies can require manual recovery. See the persistence limits in architecture.

- Open/restored tabs read local history automatically, without consent settings. Read does not mean fetch. Selected saved tab loads first; others load on selection.
- Tabs immediately show cached rows, then validate identity off-thread. Snapshots older than 30 seconds refresh on tab/window activation; Repository → Refresh is manual. No polling/preload. Loading/failure keeps rows with stale/error status; missing/replaced/inaccessible identity clears rows. Explicit reopen binds a changed location.
- **All Commits:** ancestry of local branches, remote-tracking branches, tags, current HEAD and saved stashes, including detached HEAD. Other reflog-only and unreachable history is excluded. Unborn HEAD does not hide other branches. No scope/filter selector; legacy narrower settings are ignored; scope help is accessible.
- Headerless virtualized history begins directly below repository tabs. Graph, ref badges, description, author, local date and short hash occupy fixed topology order; sorting/header-drag resizing are unavailable. Description adapts to width. Merge nodes have a hollow marker and parent help.
- Selecting a row exposes full message, author/committer, original-offset dates, hash, parents and full refs below. Row labels are bounded previews with full tooltip/details; parsed messages within the read limit remain complete.
- View offers Older/Newer, range and status; Ctrl+Page Up/Down or scrolling past a page edge navigates 200-row pages; no total commit ceiling. Cache: three visited tabs, one page each, character budget; 128 back cursors plus First history page. Other tabs retain session-only pages; the selected verified page also supplies the persisted startup preview. Eviction reloads page one. Refresh preserves page for unchanged refs/HEAD; otherwise restarts, retaining the selected hash if present.
- Loading, end/shallow boundary, empty and errors are distinct; View and Diagnostics expose limits. Read failures over retained rows show a compact diagnostic link. Failed/cancelled paging retains rows; ref/boundary changes require refresh. Missing objects are never fetched implicitly. Original Git stderr remains in session Diagnostics.
- Tab context menu: Refresh, Move left/right, Close, Close others; acts on clicked tab. Middle-click closes it. Active close cancels its read/selects a neighbor; inactive close preserves selected work. Closing never deletes repositories.

Pull shows **behind**, Push **ahead**, comparing the configured upstream from the local snapshot, even with a separate push destination. Counts load after rows. Zero is measured; dash means pending/unavailable/cancelled/failed. Help includes upstream/time/reason. Detached/unborn/no-upstream/missing-upstream/shallow cannot imply synchronization. Failed refresh retains stale counts; no server freshness claim without fetch.

Same-author peers use a muted yellow **row background**, retaining ordinary text color. Matching remains exact raw name/email ordinally and case-sensitively, without mailmap/committer; nonblank email required. Selected rows keep normal selection styling. High contrast suppresses the peer fill; accessible text explains the relation. Graph lanes use compact straight runs/rounded joins and hollow merge markers; selection preserves lane colors. Since 0.26.0, branch lanes stay fixed until their shared target; stash folding preserves visible-page size and original parent metadata.

The main footer and permanent history page-control bar are removed in 0.27.0. Installed version remains under Help; results/settings notices and limits remain in Diagnostics. Settings problems are also visible in the top-center caption. Caption glyphs are enlarged; their existing hit targets and window commands are retained. Tooltips give short action/count hints rather than prototype explanations.

## Compact caption and history (0.26.0)

File/View/Repository/Window/Help share the main window's draggable caption strip with its title and Minimize, Maximize/Restore and Close controls. Empty caption space retains standard move/double-click/system-menu behavior through WPF WindowChrome; Close follows existing shutdown handling. Other dialogs retain their native captions. The left toolbar is Open, **Fetch, Pull, Push, Commit**.

Each saved stash is one history entry connected to its base, rather than a merge plus separate index/untracked commits. Its real parents/message remain in details; the existing Stashes inspector still exposes staged/untracked content and Apply/Pop/Drop. Objects independently reachable through normal branches/tags/HEAD or a stash base remain visible. Unusual stored commits keep their ordinary history shape. All Commits retains normal history scope and 200 visible rows per page; sidebar jumps use the same folded ordering.

Branch tracks keep their lane and color until the actual shared ancestor, including consecutive merges with the same base. Wider rounded turns replace short zigzags. Independent roots, octopus and criss-cross merges and page boundaries use the same bounded graph rules. Layout remains capped at 64 occupied/reserved columns; unusually wide history reports the existing limit instead of drawing false connections.

## Operation feedback (0.25.0)

The top-center repository/branch panel keeps fixed title, status and duration positions. Timed Git actions and Stage/Unstage/Commit display elapsed seconds in a small capsule; phase text stays separate so changing digits cannot move it. The timer includes validation/execution and final reconciliation, not only the Git subprocess. A running command remains running through that reconciliation. History reads, page/ref navigation and refreshes also produce completion capsules since 0.27.0. Secondary status/diff scans stay quiet; action reconciliation keeps the original action timer.

The capsule fades in over 180 ms without restarting on each tick. At completion the capsule turns green for success, amber for conflict or failed refresh, red for failure, or neutral for cancellation, with a corresponding symbol and explicit outcome text. It holds for three seconds then fades over two; Windows animation preference/high contrast removes the fade. The caption, errors and retained Operation/commit results remain available after the capsule disappears. A new attempt replaces the old animation; switching repositories clears transient feedback instead of replaying it. Long captions/durations truncate within their slots with full tooltips/accessibility help. Details (ellipsis) and Cancel (cross) remain in reserved top-right space inside the panel.

## Workspace toolbar and appearance (0.24.0)

Repository tabs share the entire strip equally (one fills it, two split it, etc.); long names truncate with full-path tooltips. Existing keyboard selection, reorder/close menus and middle-click close remain.

Top-right **Open in** offers File Explorer, Rider and VS Code; **Console** opens Git Bash in the selected repository (also Ctrl+Alt+T; Explorer Ctrl+Alt+O). **Appearance** switches Dark/Light immediately and remembers it. Native title bars follow the palette where supported; Windows high contrast takes precedence. No Work/environment button is added.

Opening these menus performs no Git, tool discovery or filesystem preparation. Selecting a tool captures that repository, discovers a supported executable off-thread, then hands it the folder/project using literal arguments. Rider prefers a single root-level `.uproject`, then `.slnx`, `.sln`, `.csproj`; ambiguous/no matches open the folder. Nested projects are not searched. A missing tool prompts for its `.exe`, remembered in the profile; no dependency is installed. Loading/failure appears in the top-center caption without clearing history. External tools control their own lifetime; Shovel reports launch dispatch, not successful project loading, and never kills them on exit. Refresh/activation observes subsequent repository changes through the existing workflow.

## Repository sidebar (0.23.0)

Expandable **Branches**, **Remotes**, **Tags**, **Stashes** and **Submodules** use local snapshots. Lists refresh after first history, on stale activation or explicit Refresh; there is no automatic fetch, polling or inactive-tab preload. Empty groups are honest empty states; pending/failure text is explicit and failed refresh retains cached entries. Menus open from available data without preparation. The sidebar scrolls vertically; long labels ellipsize and reveal their full name on hover. Its splitter has no fixed maximum width, retaining 180 DIP for the workspace. The duplicate repository title and Local Changes entry are removed; Commit remains in the toolbar.

- **Branches:** current checkmark and local upstream counts (↑ ahead / ↓ behind). Single click locates its commit; double click or Enter switches. Right-click offers Switch, Show in history, Delete, Set upstream and Unset upstream. Existing clean-state/worktree/deletion protections apply. Counts reflect fetched local refs, not live server state; no upstream has no count, pending shows an ellipsis and removed upstream is identified.
- **Remotes:** configured remotes and their fetched remote-tracking branches. Click a branch to locate it; remote context menu Fetch performs the explicit fetch. Repository → Add remote (also the Remotes group menu) saves a name and URL/local path without contacting it. Choose Fetch explicitly, then Repository → Set upstream to select an existing fetched branch; Unset upstream is there too. Edit/delete remain deferred.
- **Tags:** click to locate the tagged commit, including older pages; no checkout or filtering. Tags pointing to non-commit objects have no history row and are omitted.
- **Stashes:** inline saved messages/selectors; click selects the stash commit with its label in history. A hollow square distinguishes its one base connection. Independently reachable index/untracked commits and real stash-only base ancestry remain visible; they do not turn the stash into a merge. Right-click Apply/Pop/Drop uses real existing actions; Drop confirms the actual saved target. These quick actions restore worktree content without restoring staged state. Repository → Stashes retains detailed inspection/Create and the Restore staged state option.
- **Submodules:** indexed gitlinks, including uninitialized ones. Click opens an initialized child as another repository tab. Right-click **Initialize / Update** checks out its recorded commit (may download); **Sync URL** copies configured URL changes. No force, recursive update, implicit stage or commit. Missing/uninitialized paths explain how to initialize; dirty collisions/active child operations fail with retained output. Add/remove/deinit/recursive/remote-latest management is deferred.

Navigation retains useful history while locating an uncached commit asynchronously, selects it and scrolls into view. All Commits returns to the first page after a jump. Read/time limits report failure rather than silently omit data; [architecture](architecture.md#repository-sidebar-0230) owns limits, including the current 256-saved-stash bound. A jump starts a fresh graph lane layout for that page; normal Older paging carries it forward.

## Committed changes (0.15.0)

Select a history commit and open the lower **Changes** tab. Its file list and colored/numbered unified diff load asynchronously, starting with the first file. The selected commit's metadata remains in **Commit**. Compare with parent 1 by default; merge commits offer each parent explicitly. Initial commits compare with the empty tree, while incomplete/shallow ancestry reports a limitation rather than pretending to be an initial commit. Empty commits show zero changed files. Binary files show stored old/new byte sizes; submodules remain pointer-only.

Amber **M**, green **+**, red **−** mark modified, added and deleted files here and in Local Changes; rename arrows and tooltips expose original names. Selection uses normal selection text; high contrast uses system colors and retains symbols/accessibility labels. Working files/index do not supply committed content. Raw patches/errors remain in the output expander. Changed commit/parent/tab cancels obsolete reads; same-comparison refresh failure retains useful content with an error. No preload/polling or startup history delay. Resize the lower panel/file-list divider; Changes reserves usable space at minimum window size.

## New local repository (0.21.0)

File or Repository → **New repository…** opens a fixed-size, close-only dialog immediately. Enter an absolute destination or Browse to an existing folder, and optionally enter the initial branch. Blank uses Git's default. Opening/editing options performs no Git or filesystem preparation; Create closes the dialog and starts validation/execution with top-center progress, Cancel and retained Operation output.

Create supports missing or populated ordinary folders, preserving files without staging, committing or adding a remote. The created worktree opens automatically with an honest no-commits state and subsequent local file count. An existing worktree root opens unchanged; the initial-branch option does not rename it. Failure retains the previous tab and explains the cause. Cancellation/partial failure keeps created files and opens the result only when fresh inspection confirms it is usable; it is not rollback. [Supported destinations and limits](architecture.md#local-repository-initialization-0210).

## Local Changes / Commit (0.13.0)

**Confirmed:** the toolbar's **Commit (count)** and sidebar Local changes open the same resizable owned modal, leaving history/page/selection visible behind it. A partially staged path counts once. Count is a local snapshot, read only for the selected tab after history is published, then refreshed with history or after actions; it can become stale after external edits. A pending/failed count is unknown, never a fabricated zero. No polling or inactive-tab preload. This replaces the embedded Local Changes workspace and its no-startup-status rule.

Left: unstaged/untracked/conflicted files above staged files, each with one action button. Select files (Ctrl/Shift multiselect) for **Stage** or **Unstage**; hold Shift while activating those same buttons for **Stage all** / **Unstage all**. Ignored files are excluded; force-add is unavailable; Discard is in the file menu. Partially staged files appear in both lists with independent diffs. Rename origins and exact paths remain in help; control characters are escaped in visible rows.

Right: numbered colored unified diff above subject/description and **Amend**. Added/removed markers and accessible descriptions supplement colors; selection retains normal styling. Ignore whitespace, bounded whole-file context and previous/next hunk navigation are real read options. Binary files show old/new byte sizes; submodules show pointers only. Oversized, non-UTF-8, linked paths, nested directories, unchanged content and missing objects have explicit notices; no implicit network retrieval. Image previews and general text side-by-side diffs remain deferred; conflicts use the dedicated editor above. Automatic reads suppress filters/external diff/textconv; filter/LFS output can differ from normal Git. Explicit whole-file staging honors configured clean filters.

**Selected hunks (0.14.0):** select any line inside a change block, or Ctrl/Shift-select several blocks, then use the single **Stage hunk(s)** / **Unstage hunk(s)** button beside the diff. This selects complete blocks, not individual lines; accessible row descriptions identify each block. Ordinary tracked text modifications are supported with normal diff options; unavailable types/options explain why and retain whole-file actions. The command preserves working files, unrelated index edits, draft and selected file. It clears block selection after refresh; stale previews fail visibly and reinspect without retrying. [Architecture](architecture.md#selected-hunks-0140) owns exact restrictions and race limits.

**Commit** records the staged index with the entered message. **Amend** can preload the previous message when the draft is empty; execution confirms the actual branch/hash because it rewrites that tip. Hold Shift for **Commit and Push**, using the attached branch's configured upstream; a missing upstream disables that variant with a reason. No force push. Push failure retains the new commit and reports the two outcomes separately. Conflicted index entries block commit; after resolving and staging files in the conflict editor, a pending merge can be committed here. Other sequence recovery uses the sidebar/Repository controls described below.

Opening binds cached rows/draft immediately, then reads after layout; it never fetches. First load, clean lists, active reads/actions, stale rows, failures and conflicts remain distinct. Refresh/Cancel is one modal control. Failed/cancelled status reads keep useful rows but disable writes until a fresh context is available. New selection cancels obsolete diff work. Static action results and retained output stay in the modal; command duration stays at top center. Closing cancels/drains/reinspects, without claiming undo. History refreshes after the modal closes following a mutation.

**0.16.0 binary comparison:** old/new cards show file extension, a generic file icon and size below in binary units plus exact bytes. Added/deleted sides say “No file,” distinct from an empty file. LFS labels and logical asset sizes come from bounded local pointer metadata; no asset download or helper is needed. Text files keep the normal numbered diff; in-app image preview remains deferred. The same cards serve historical Changes.

**0.16.0 file-action feedback:** file Stage/Unstage updates affected rows from real Git status before a wider background scan. Keep selection on the same path when its side changes. A short trailing delay coalesces successive actions; cached rows remain usable, newer generations supersede old reads and a failed full refresh visibly marks retained rows stale. Bottom activity bars and the ticking commit footer are removed. Static results/errors, Refresh/Cancel, in-memory phase timings and command-only top-center duration remain. Ordinary background work adds no throbber or polling.

File selection and last output remain session-only. Draft persistence in 0.22.0 supersedes the old session-only draft policy; successful commit clears the submitted draft, failed commit retains it. Since 0.27.1, temporary file-sharing/lock failures get short background retries without delaying modal opening. Persistent failures retain the save/load problem and editor text; a competing profile's newer draft is preserved, with this window's text in its own recovery copy when storage permits. [Storage and recovery limits](architecture.md#integrated-branch-management-and-drafts-0220).

## Local Changes file menus (0.19.0)

Right-click a staged or unstaged/untracked/conflicted row for **Stage/Unstage**, **Stage all/Unstage all**, **Open**, **Show in File Explorer**, **Copy path** and **Copy full path**. A selected row keeps its multiselection; a different row becomes the single target. Blank space has no file menu. The menu captures its targets, so refresh cannot silently redirect a command; reopen a stale menu. Opening starts no Git/filesystem preparation. A right-click selection's diff waits until the menu closes; existing useful content remains visible.

Stage/Unstage operates on the captured selection; the explicit All entry uses all rows on that side. Shift does not expand these menu commands. In a file grid, **Enter** stages/unstages selected files, **Ctrl+Shift+Alt+S** acts on all listed files, **Ctrl+C / Ctrl+Shift+C** copies relative/full paths, and **Ctrl+Shift+Alt+O** opens one file. **Shift+F10 / Menu key** uses the native context menu; Enter inside the popup activates its highlighted item. Shortcuts are scoped to file grids/menus, leaving message editing and diff selection alone.

Open/Explorer require one selected row. Open uses the **current working file** and its Windows default application, even from the staged list; it does not export staged content. A deleted row explains why Open is unavailable; Explorer selects an existing file or opens the nearest existing containing folder. Copies preserve exact paths, one per line. Clipboard/default-app/permission failures are localized without logging contents. File handoff is asynchronous and bounded, but an already sent Windows request cannot be retracted. External application lifetime is not tracked; refresh after editing.

### Selected-file actions (0.20.0)

**Discard changes…** (Delete in a file grid) opens immediately with the repository, HEAD and exact selected paths/rename endpoints. Cancel is the default. Unstaged tracked files restore from the index, preserving staged content; staged selections restore **both index and working files to HEAD**, including unstaged edits at those paths. Selected untracked files and unborn staged additions are permanently removed, without a recycle bin or promise of undo. Conflicted files, submodule directories and linked paths are excluded. Submission rechecks state; cancellation/failure can leave partial results, which are reinspected without automatic cleanup.

**Stash selected files…** shows the selected paths and optional message. It saves both staged and unstaged content for those paths, plus selected untracked files; unrelated index/worktree content stays in place. Inspect/apply/pop/drop the result in Stashes. Git can reject applying into a dirty index. A stash saved before cancellation remains available; partial index cleanup is reported separately, never retried automatically.

**External Diff** runs the configured Git diff tool for one file and the selected staged/unstaged side; an untracked file compares against empty content. Missing configuration and tool failures are explained after submission. The command remains tracked until its process exits or is cancelled; a tool that detaches itself cannot be tracked afterward.

**History… / Blame / Timeline…** opens a resizable window before reading. History follows the captured HEAD's **first-parent file ancestry and renames**, with 200-row pages and commit metadata/real changed-file diffs; it is distinct from main-window All Commits. Blame shows line, author, date, hash and committed text at that tip, excluding working edits. Refresh rereads the captured tip; reopen for a new HEAD. Loading retains existing rows; no history, missing objects, unsupported/binary blame and bounded-read failures are explicit. No automatic fetch.

**Ignore** offers literal selected paths or their extensions throughout the repository, for untracked files only. It appends root `.gitignore` rules without removing files or untracking content; nested overrides are reported. **LFS** adds exact-path tracking/reset rules to root `.gitattributes`, preserving other rules. Tracking needs existing Git LFS/filter setup; neither action installs hooks, migrates history, downloads or restages content. Review/stage the rule file and affected files yourself; more-specific attribute overrides are reported.

**Save as Patch…** exports selected staged or unstaged/untracked changes, including binary patches, through a save picker with overwrite confirmation. It never stages/discards source files; failed/cancelled writes preserve an existing destination. Limits and configured-filter differences are described in [architecture](architecture.md#selected-file-actions-0200). Menu actions retain captured targets and Shift does not expand their scope.

## Stashes (0.18.0)

Repository → **Stashes** opens a resizable owned window over retained history; the sidebar now lists entries directly. Opening uses the tab's cached list/message immediately, then refreshes local stashes in the background; no remote queries, initial Git prerequisite or polling. Select an entry for its full hash/message and stored files/diffs: all tracked changes, saved staged changes, saved unstaged changes, or saved untracked files. Text/binary/LFS views are shared with historical Changes; current working files never supply saved content.

**Create stash** accepts an optional message and unchecked **Include untracked files**. It saves staged/unstaged changes and removes those saved changes from the worktree; ignored files and submodule contents are excluded. **Apply** restores the selected entry and keeps it. **Pop** applies first and removes that entry only after successful application with no detected conflicts and an unchanged stash list. Unchecked **Restore staged state** controls both Apply and Pop. **Drop** confirms the exact repository, stash selector/hash/message and loss of the saved entry; it does not change working files and offers no guaranteed undo.

Empty lists, loading, stale refresh failures and command results are explicit. Selection follows the saved hash across refreshed ordinal numbers; useful list rows/draft survive failures and modal reopening during the session. Failed list reads disable selected-entry writes until refresh succeeds. Commands expose top-center duration/Cancel and retained in-memory output in the modal. Close cancels/drains work; history/counts refresh afterward. No automatic retry or rollback.

Conflicts or incomplete restoration keep the stash; partial Pop success explicitly says when changes were applied but removal was not confirmed. Do not blindly apply again. Resolve and stage files in Local Changes; stash conflicts have no Continue/Skip/Abort sequence. [Architecture](architecture.md#stashes-0180) owns limits and external-client races.

## Conflict recovery (0.17.0)

Detected merge, rebase, cherry-pick and revert operations expose **Continue**, **Abort**, and **Skip** when a current replayed commit is known. Controls appear in the sidebar and Repository → Recovery from the cached snapshot; opening them performs no Git or filesystem work. Reopening a repository rediscovers Git's state rather than replaying a saved action.

Open **Local Changes**, use Resolve conflict for each file and stage the resolutions, then close that modal and choose **Continue**. Continue uses Git's existing message; it refuses unresolved index entries. A subsequent conflict keeps recovery controls available. Abort confirms the repository, branch and observed original tip; Skip also identifies the current commit and warns that resolution edits will be discarded. Git may fail to restore all earlier uncommitted changes. Neither action promises complete undo.

Commands use top-center progress/Cancel and retained Operation output. Existing history remains visible. Failures/cancellation re-read state; no automatic abort, stash deletion, lock cleanup or retry. A conflict while restoring an auto-stash remains a conflict even after the sequence ends: use Local Changes and inspect the preserved entry in Stashes. Mailbox (`git am`), ambiguous/oversized state and interactive todo/message editing remain unsupported. Hooks, signing and existing rebase exec steps remain enabled; unsupported input still requires existing tools. Sequence recovery remains separate from the per-file conflict editor.

## Explicit branch and remote workflows (0.8.0)

Branch split button defaults to New branch (name, start reference, optional checkout); arrow lists local branches for switching. Start points bind observed commits. New branches do not inherit tracking; Push can set upstream. The commit editor can create an initial commit in an unborn repository.

**Confirmed 2026-10-28:** operation dialogs open fully editable from the tab's local snapshot, with no Git queries, remote checks or preparation on opening. Pull, Fetch, Push and branch options are fixed-size, with only Close in the title bar (no resizing/minimize/maximize); long content scrolls. Remote names include configured but unfetched remotes; branch choices reflect locally known refs. Use explicit Fetch to discover new remote branches, or Refresh after external local configuration changes. Target help explains that execution uses the named remote's current configuration. Fetch chooses one/all remotes; Push chooses source, remote and destination with unchecked all-tags/set-upstream options.

Ordinary UI interactions must respond promptly, without a loading prerequisite. Necessary command validation/execution runs after submission with visible progress. Explain and discuss any unavoidable new UI wait rather than silently introducing one. Pull still fetches its selected branch before integrating; merely opening any dialog never fetches.

**Confirmed 2026-10-28:** Pull defaults to **Rebase**, with **Merge** as the alternative. Rebase replays local commits and changes their hashes; Merge preserves them and creates a merge commit for divergence. An initially unchecked **Automatically stash and reapply local changes** checkbox uses Git's tracked-file auto-stash. Untracked files remain in place; staged content can return unstaged. Git protects ordinary collisions/dirty state; Shovel no longer rejects all untracked files. Conflicts retain Git recovery state and original output, with in-app Continue/Skip/Abort and retained output; a conflict during stash restoration must not appear as successful completion. See Conflict recovery above; conflict editing remains deferred.

Pull preserves existing sparse-checkout patterns and only updates superproject references to submodules. Submodule contents are neither updated nor included in auto-stash; use existing tools for those workflows. Branch switching in these repository modes remains deferred.

Normal action buttons execute without an additional trust checkbox. **Submitting Pull options closes the options modal immediately**; the same shared flow applies to Fetch/Push/branch actions. The main window shows the actual phase (checking targets, fetching, checking fetched commit, updating local branch), elapsed time and indeterminate progress at top center. Existing history stays visible/selectable. Cancel is available there and in Repository; cancellation is unavailable during fresh inspection. Missing targets/dependencies disable actions with localized help.

Operation details opens below with the repository, full result explanation and bounded original/redacted Git output. Success/failure/cancellation persists at top center and in details after reinspection. A failed refresh is distinct from Git success. One last result is kept in memory, only shown on its repository tab; cancelling a later options dialog preserves it. No percentage is invented. Authentication prompts are owned by the main window after options close. Options/execution/reconciliation still blocks tab navigation/new Git actions/window close; attempting close shows operation details. Cancellation stops owned work and reinspects; it does not undo changes. Related caches become stale after every attempt; retry uses fresh targets. No automatic retries/abort/lock removal; auto-stash occurs only when selected for Pull.

Badges distinguish local/tag/remote refs with original drawings. Check marks current tip, including detached HEAD. Only a local branch and its configured same-tip upstream share a local-name badge with remote icon; unrelated remotes stay separate. Full refs and relationship appear in help/accessibility. Remote icons describe local tracking tips.

## Authentication prompts (0.9.1)

Explicit Fetch/Pull/Push can open owned Git authentication prompts through Git/bundled OpenSSH askpass. Show operation/repository, plain request and Waiting for authentication input. Replies are masked, including usernames/host-key answers; host acceptance is explicit after fingerprint verification. OpenSSH confirmation has Approve request; Enter defaults to Cancel. Informational prompts last until helper exit/cancel.

Closing/cancelling stops and reconciles the operation; timeout is two minutes, with bounded requests and connection failures shown. Shovel saves no credentials/prompts; configured Git helpers may store credentials and OpenSSH may save accepted keys. Browser/GCM sign-in, signing, custom SSH and interactive hooks still need existing tools. Live provider/focus/input behavior is unverified. History never authenticates.

## Interface and remaining workflows

**Confirmed 2026-10-28:** default dark client area inspired by the supplied screenshot: distinct chrome, repository tabs, sidebar, history and details surfaces. Shared hover/pressed/disabled/selection/focus states extend to existing dialogs; lower tabs use an orange underline. [Theme](theme.md) owns colors and conventions. Shovel title bars now match the dark chrome through native Windows styling, with system high-contrast fallback; pickers/message boxes remain OS-owned. 0.24.0 adds live Dark/Light selection and matching native chrome; further polish remains deferred.

Current frame: File/View/Repository/Window/Help; Open/Commit/Fetch/Pull/Push; repository/branch/operation timer and Branch menu; repository tabs; Local changes modal entry/All Commits sidebar; history; resizable details. Repository owns secondary actions, Git selection and Diagnostics. Icon: selected front-facing silver shovel, cropped shaft, blue circle.

Unfinished Explorer/Terminal/Editor/Tools buttons, sidebar destinations and force options are omitted. Stashes is available in the sidebar and Repository menu. Historical file-tree browsing, other mutations/tools remain pending. Conflict editing is delivered above. Future controls ship with functionality. Filtering is revisited only when requested.

**Proposed refinements:** context menus target the clicked object while preserving a containing multiselection; shared commands serve menus/toolbar/shortcuts. Large rebase/merge editors show real plans/context. Dragging proposes an operation, never rewrites immediately. The previously proposed separate embedded Changes workspace is superseded by the confirmed modal design above.

| Target workflow | Behavior and failure states |
| --- | --- |
| Open/init/clone | Path/remote choice; async progress/cancel; distinct missing Git/invalid path/empty state. Preserve partial clone destinations for explicit recovery. |
| History/files/diffs | Incremental real metadata/tree/content; distinguish no matches, shallow/missing objects, binary and oversized content from errors. |
| Stage/commit/amend | Real file/hunk/index changes; preserve draft/selection; show stale-hunk, lock, hook/signing failures with current Git state. |
| Branch/network/stash | Concrete refs/remotes/scope; explain divergence, rejection, auth/network failure/conflicts; no destructive retry. |
| Conflict/sequence | Persistent context banner; valid resolve/continue/skip/abort. Reopen reconstructs observed state; dismissing a dialog never ends/replays a sequence. |
| External tools | Track sessions, reconcile edits and retain selection; launcher exit alone may not mean editor completion. |
| Close/tray | Optional explicit close-versus-exit setting; no surprise autostart; hidden does not mean idle. |

Reference: original attachment unavailable; 2026-10-25 screenshot confirmed headerless history. [Fork research](research/fork-ui-observations.md) separates old official screenshots/animations from inference. No proprietary assets or broader parity claim.

## Update experience (confirmed)

0.30.0 implements Windows packaging and asynchronous signed update checks/downloads; actual installed rollout validation remains open. Public source Shovel uses MIT; downloads use public shovel-releases under the owner’s personal account (GlebHmarsky; repositories private during preparation). See [distribution](distribution.md) for setup. Optional periodic checks and manual checks expose installed/available version, channel, notes and status. Download/verify with progress/failures; allow postponement or explicit **Restart and update**. Download never authorizes installation. Apply only after final idle check; preserve settings/tabs/drafts/recovery data. Unavailable server must not block startup/Git.

States: checking, current, available, downloading, verifying, ready, postponed, blocked by work, installing, failed, recovered. [Update requirements](requirements.md#updates-and-distribution) owns install/authenticity/failure acceptance.
