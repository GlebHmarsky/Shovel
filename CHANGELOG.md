# Changelog

## 0.7.0 — 2026-10-25

- Place the history table directly below repository tabs, with compact paging/status below it. Remove scope/filter controls; All Commits always includes local branches, remote-tracking branches, tags and the current HEAD. Older current-branch preferences no longer restrict the view.
- Restore File/View/Repository/Window/Help menus, the top action toolbar and sidebar destinations. Move repository, branch and read timer into the top-center block. Unimplemented Git/tool/sidebar actions are visible, disabled prototypes with localized explanations.
- Show real behind/ahead counts beside Pull/Push, calculated after history appears from the configured upstream's local snapshot. No network access or actual pull/push is added; unavailable, incomplete and failed comparisons show a dash rather than a guessed zero.
- Add disposable upstream/divergence tests, cancellation/failure checks and offscreen workspace verification. Existing tab caches, selection, author highlighting and read safety remain.

## 0.6.0 — 2026-10-25

- Remove local-history approval questions and approval management. Opened and restored repositories load automatically; remembered identity detects replacements, with no permission to run hooks, scripts or remote fetches.
- Launch Git directly inside a Windows Job Object, removing per-command .NET helpers. Prefer Git for Windows’ documented native entry point; combine HEAD/ref inspection and reuse its snapshot for initial/refresh reads. Preserve bounded output, cancellation, ownership checks and post-read validation.
- Add repository-tab context menus for refresh, move left/right, close and close others, plus middle-click closing. Active-tab close cancels its read; inactive-tab close preserves active work.
- Migrate settings to schema 4; retain previous fingerprints as replacement stamps. Add headless tab/process checks and a packed 100,000-commit benchmark. No installer, updater, network features or interactive launch.

## 0.5.0 — 2026-10-25

- Automatically load the selected approved repository at startup and restore cached history on tab switches. Preserve per-tab scope, page, selection and scroll in a bounded session cache; refresh asynchronously without clearing useful rows.
- Remember revocable approval tied to filesystem identity; migrate old settings without silently approving saved paths. Check identity before automatic Git work and stop on replacement/missing paths.
- Reduce repeated Git/helper startup with combined metadata queries, bounded parallel HEAD reads, fewer ref queries and source-generated process protocol serialization. Add repeatable headless timing evidence and bounded in-memory diagnostics.
- Move implemented secondary actions into Repository; remove prototype toolbar/sidebar/details controls. Highlight same-author peer text in light yellow with explicit accessible semantics and high-contrast fallback.
- Keep Git functionality read-only. Status, diffs, mutations, network and updater remain deferred; no interactive launch or desktop control.

## 0.4.2 — 2026-10-25

- Add the user-selected front-facing silver shovel on a blue circle as the executable and main-window icon. Include transparent source artwork and a multi-size Windows icon.

## 0.4.1 — 2026-10-25

- Remove the history column-heading row following the supplied Fork screenshot; keep aligned commit columns, localized cell accessibility help and row descriptions. Scope, paging and loading indicators remain visible.
- Header drag resizing is no longer exposed; description width continues to adapt to the window. Git behavior is unchanged.

## 0.4.0 — 2026-10-24

- Default to All Commits: local branches, remote-tracking branches and tags, with branch/tag labels. Current branch / HEAD remains a separate read-only scope, including detached HEAD.
- Replace the 2,000-commit ceiling with older/newer pages of 200 rows, visible row range, scope, filter state and resource limits. Keep graph lanes connected across pages without retaining old messages.
- Reserve more space for history at minimum window size; keep the bottom details panel resizable and scrollable.
- Show complete selected messages and metadata within the bounded read; retain the current page on paging errors/cancellation. Require refresh when refs or traversal boundaries change.
- Verify disposable multi-branch, merge, annotated/nested tag, packed-ref, shallow, detached/unborn and 2,005-commit histories headlessly. No interactive app launch, Git mutations or network features added.

## 0.3.1 — 2026-10-24

- Name the application Shovel: localized window/dialog title, executable and product metadata.
- Preserve the existing settings location and profile mutex so saved layout/tabs remain compatible; internal project names stay unchanged.
- Refresh project documentation and rebuild the local preview after the user-requested cleanup of old Build packages. Git capabilities are unchanged from 0.3.0.

## 0.3.0 — 2026-10-23

- Show real HEAD history in a virtualized table: connected graph nodes on the left, then description, author, local commit date and abbreviated hash. Merge nodes have a second ring.
- Select a row to inspect its message, author/committer identities and dates, full hash and parents in the bottom panel.
- Read 200 commits initially; load older commits from the same observed tip, up to 2,000. Bound output, graph lanes and long message previews; distinguish unborn and shallow history. Refresh explicitly after external changes.
- Verify disposable linear/merge/shallow histories, signature-helper suppression, stable loading, stale-result rejection and headless WPF rendering/virtualization. No desktop control or interactive launch in this delivery.
- All-branch history, search/filter, file status, changed files/diffs and mutations remain deferred.

## 0.2.0 — 2026-10-23

- Open trusted local repositories and inspect real branch/HEAD, worktree/common-directory identity and interrupted operation markers. Refresh manually after external changes.
- Restore repository tab order/selection without restoring trust; reorder and close tabs. Schema 1 layout settings migrate to schema 2 with a backup on save.
- Add bounded asynchronous process execution, separate stdout/stderr streaming, timeouts, cancellation and Windows job containment through a waiting helper process.
- Add isolated process/Git checks and a reproducible, ignored `Build/0.2.0-win-x64` preview package.
- File status, history, diffs, mutation workflows, interactive authentication, installer and updater remain deferred.

## 0.1.0 — 2026-10-23

- Establish the C# / WPF / .NET 10 MVVM shell, English resources, startup/shutdown, safe diagnostics and versioned layout settings.
- Verify the foundation with 17 checks and actual Windows 11 GUI inspection.
