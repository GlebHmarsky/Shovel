# Changelog

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
