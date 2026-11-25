# Changelog

## Rust 0.52.8 - 2026-11-26

- Mark new Local Changes files with a green plus. Show one centered preview for an added binary or LFS file, and keep both previews with centered captions for modified LFS files. Added text uses the existing green diff lines.
- Press Home in History to return to the newest commit from any page.
- Show conflict sources with verified operation roles, refs, commits, authors and file status. Choose Use Ours or Use Theirs, or select both sides to Merge in the configured external tool. Open the Shovel text editor separately when available.

## Rust 0.52.7 - 2026-11-26

- Stop the Local Changes files divider at the fixed-height commit editor and scroll long descriptions inside the editor.
- Limit new commit subject input to 72 characters, flag subjects longer than the recommended 50, and preserve existing drafts.
- Select all files in the focused Staged or Unstaged list with Ctrl+A. Ctrl-click toggles files, Shift-click selects a range, and Ctrl+Shift-click adds a range. Ctrl+A in a commit field selects its text.

## Rust 0.52.6 - 2026-11-22

- Shift-click selects a range in Local Changes. Ctrl-click toggles files and Ctrl+Shift-click adds a range.
- Selected files stay blue in the active window and turn gray when the window loses focus.
- Show a single Unstaged count above the file list.

## Rust 0.52.5 - 2026-11-20

- Left-align text diffs and their line numbers. Hide inline whitespace, layout, hunk and change-navigation controls.
- Open selected diffs while Local Changes refreshes. Read the patch and line counts together and skip hunk preparation for previews.
- Overlap scoped status and index reads after staging.

## Rust 0.52.4 - 2026-11-20

- Focus Discard by default. Use Enter to confirm or Left and Enter to cancel.
- Keep the reading position during history refreshes and reveal incoming commits when already at the top.
- Restore gray stash markers at reachable bases. Keep detached-base stashes in the sidebar.
- Retain Pull and Push counts while a refreshed comparison is pending. Dim incoming rows slightly and use subtle, lighter-bottom avatar gradients.
- Keep whole-file staging and history actions available during routine reads. Skip selected changes that have disappeared.

## Rust 0.52.3 - 2026-11-19

- Right-click a commit to create a branch or tag, merge or rebase the current branch, reset, check out the commit, cherry-pick, or revert.
- Save a commit as a patch, compare it with tracked Local Changes, or copy its details. Branch submenus provide pull, push, fast-forward, rename, and copy as applicable.
- Dim incoming commits, their graph rows, and their ref labels while keeping local outgoing commits in their usual colors.

## Rust 0.52.2 - 2026-11-19

- Keep saved stashes in the sidebar. Give them the same row and icon geometry as branches, with gray markers.
- Scroll across history page boundaries in either direction.
- Open any repository tab in File Explorer or the console from its context menu.
- Show up to two initials on brighter author tiles with a subtle gradient.

## Rust 0.52.1 - 2026-11-19

- Retain Pull and Push counts while reading history and upstream state. Preserve cached Local Changes counts on temporary status errors and keep the selected stash comparison during list refresh.
- Remove the Local Changes branch and status strip. Keep the commit controls below the description and show the character count on the subject only.
- Label submodule tabs as `Project :: Submodule`.
- Select multiple stashes in the sidebar and delete the reviewed selection after confirming its count.

## Rust 0.52.0 - 2026-11-19

- Development build with the commit message guides, direct conflict resolution, and stale diff cleanup.

## Rust 0.51.10 - 2026-11-19

- Add column 72 guides and live character counts to the commit editor. Enter moves from the subject to the description, and Ctrl+Enter commits the current draft.
- Clear a retained conflict diff when a fresh status scan confirms the conflict is gone.

## Rust 0.51.9 - 2026-11-19

- Show paused rebase controls below repository tabs and hide them while Shovel runs the rebase.
- Open the resolver when selecting a conflicted file. Choose both sides to merge, one side to resolve, or neither to restore the common base.
- Show conflict markers and Git LFS sizes, and keep unresolved files out of ordinary staging.
- Show progress animation only in the top repository header and save commit drafts quietly.

## Rust 0.51.8 - 2026-11-19

- Remove the strip above the menus in restored windows and reduce the height of the menu and toolbar rows.

## Rust 0.51.7 - 2026-11-19

- Open options on the first Fetch, Pull, and Push in each repository. Confirm the action once to use the saved choices on later toolbar clicks.

## Rust 0.51.6 - 2026-11-19

- Use square, contiguous repository tabs on a dark strip with left-aligned markers and thin dividers.
- Put recent repositories in File, window controls in Window, and release notes and keyboard shortcuts in Help.
- Add Repository menu shortcuts and commands for tags, linked worktrees, patch application, bisect, Git Flow features, and Git LFS.
- Warm restored tabs one at a time after the active repository is ready, including the initial commit comparison.

## Rust 0.51.5 - 2026-11-19

- Use matching arrows for Fetch, Pull, and Push, with a hollow Fetch arrow and an upward Push arrow.
- Show cached Local Changes rows while Git verifies the worktree. Use one button per file group and hold Shift to switch to Stage all or Unstage all.
- Keep the last diff visible until its replacement is ready. Show full relative paths in the file list and move manual refresh to F5.
- Show old and new binary file extensions, compact and exact sizes, and LFS badges. Show text controls only for text diffs.

## Rust 0.51.4 - 2026-11-19

- Label Open In and Console, make the History and Local Changes rows clickable across the sidebar, and keep the compact repository header in place when Update Shovel appears.
- Open Activity below the repository header in a movable, resizable window with a run list and full output.
- Click Update once to download, verify, and restart when current work allows it.
- Show saved stashes with full-size gray commit markers. Keep unusual stash entries accessible from the Stashes sidebar.

## Rust 0.51.3 - 2026-11-19

- Ask short confirmation questions with the affected targets and direct action buttons.
- Keep progress on the branch line in the repository header. Show the selected repository's changed-file count on Local Changes.
- Give Commit a full-width summary and file list. In Changed Files, show a color-coded path list beside the diff and remember the divider. Refresh lives in the Repository menu and on F5.

## Rust 0.51.2 - 2026-11-18

- Draw each saved stash as a small gray marker beside its original base commit. The stash connection ends at that commit and regular history keeps its Git topology, lane colors, and connections.

## Rust 0.51.1 - 2026-11-18

- Start reading and selecting commits while Shovel checks the current refs. Paging and repository actions become available after verification.
- Keep history selection, scroll position, and comparisons when moving between repositories or reopening recently closed tabs.
- Keep Local Changes rows, selected files, diffs, and drafts during tab switches. Windows file notifications refresh the affected repository in the background.
- Switch and close tabs while repository-owned dialogs are open. Accepted Git commands continue on their original repository, and Shovel waits for them before exiting.
- Give visible history and Commit reads priority. Reuse unchanged history and avoid extra Git reads for Local Changes and Remotes.
- Check for updates when opening Updates and keep verified downloads when selecting **Later**. Respect configured SSH and foreground credential manager settings.

## Rust 0.51.0 - 2026-11-18

- Check the signed update feed when opening Updates from the toolbar or Help. Use **Later** to close the dialog and keep a verified download ready.
- Honor Git’s configured SSH command and variant and the credential manager’s interaction settings for foreground authentication. Automatic Fetch stays noninteractive.
- Skip obsolete Git reads and automatic Fetch requests that are cancelled before Windows starts them.
- Reject a dangling Windows junction at `commondir` during repository identity checks, including when it appears during a read.

## Rust 0.50.23 - 2026-11-18

- Use Refresh or F5 in Local Changes to recover after a draft read or save fails. Shovel keeps newer saved text and can confirm a draft that was already written before its worker stopped.
- Retry an interrupted checkout review without closing it or giving up its place in the repository queue. Shovel still checks the original worktree, branch and commit before accepting checkout.
- Keep the verified update available for another Restart if the background restart checks stop unexpectedly.

## Rust 0.50.22 - 2026-11-18

- Navigate the vertical Settings categories with Up, Down, Home and End. Tab enters the selected form, and Windows accessibility exposes each category and its associated page.
- Validate both global Git identity fields before writing. Keep invalid edits available for correction, support the stated Unicode character limit, and recover the form after an interrupted request.
- Check worktree identity, unfinished Git operations and index conflicts before restarting for an update. An active bisect postpones restart, and a large reference set no longer makes a clean worktree block it.
- Report interrupted History work and finish pending Git requests with a failure when their repository could not open. The current History page stays visible after interrupted paging or reference navigation.

## Rust 0.50.21 - 2026-11-18

- Report a failed Git worker launch in Activity and release its queue so a separate linked worktree can continue.
- Return interrupted update checks and downloads to a retryable state. Keep a verified update available when a background check fails.
- Reject a linked worktree identity if its `.git` or `commondir` pointer changes while Shovel checks where the operation would run.

## Rust 0.50.20 - 2026-11-18

- Protect accepted Git work during an update restart. Shovel pauses command dialogs while closing and checks the queue again before exiting. A competing Windows process that has already exited now releases the restart guard even if it returned status 259.
- Open configured editors or Git Bash while Git is starting without letting the tool hold Git's input or output pipes open.
- Mask authorization headers, cookies, password and token fields, and password-bearing URLs in Git diagnostics while retaining the surrounding error text.

## Rust 0.50.19 - 2026-11-18

- Reopening Shovel restores Restart when the current signed update is already verified in the installation cache. If Windows cannot start the update helper, Shovel stays open with the package ready for retry or installer repair.
- Coordinate repository mutations across Windows sessions while retaining the original per-session lock for the C# client.
- Report a Git worker that loses its result and refresh the repository before retry. An automatic Fetch worker failure leaves accepted user commands queued.
- Prepare Windows pipe workers before launching Git. If Windows rejects the process start, Shovel closes the unused pipes and returns the launch error.

## Rust 0.50.18 - 2026-11-18

- If saving settings fails during an update restart, Shovel stays open with the update ready and explains how to retry.
- Keep the verified update package readable and protected against replacement through restart handoff. A package in use gives a retryable cache error.
- Keep earlier queued requests in order when a deferred action fails its preparation checks. Activity reports the failure at the request’s turn.
- Show mixed-direction repository paths correctly in Activity, authentication prompts and failure details. Hover an Activity row for its full path and copy the exact path from mixed-direction failure details.

## Rust 0.50.17 - 2026-11-18

- Settings keeps its categories on the left and groups related controls under section headings. Preview unified and side-by-side diffs at the selected font size before saving. Edit application paths together in Tools. Narrow windows stack labels above fields and keep Save and Cancel in reach.
- Since the last Beta, History can load author pictures using the saved provider. Activity shows Git exit results and filters manual and background commands by repository.
- Recover unsaved commit messages on exit and reopen failed commits in their original worktree. Repository tabs distinguish matching names and restore the selected repository. File History and Stashes remember their window sizes.

## Rust 0.50.16 - 2026-11-18

- Hold Shift when clicking Commit, or press Ctrl+Shift+Enter, to commit and push that exact commit to the configured upstream. A rejected push keeps the local commit and explains what happened.
- Scroll large Local Changes lists without laying out every row, keep files selected as they move between Staged and Unstaged, and see modified files update before the wider scan finishes. Failed refreshes keep the last rows visible and disable stale actions until a successful refresh.
- Queue Pull and Push while a repository opens. Accepted work continues after its tab closes, and automatic Fetch allows staging to proceed. Localized commit dates and accessible file and history rows make changes easier to review.
- Open the same repository through different Windows path spellings. Preserve its drafts and pending work, and keep an old commit draft separate when a different repository replaces the selected folder.
- Keep Git output responsive during Clone and hooks. Show redacted diagnostics for interrupted reads and give the masked AskPass response keyboard focus immediately.
- Show download progress and version-specific release notes. Retry interrupted or damaged downloads, keep the highest verified version for each channel, and recheck the signed offer and open worktrees before restart.

## Rust 0.50.15 - 2026-11-18

- Keep an autostash and its detached former branch tip on the shared parent's history lane. Route the newer commit's edge around them to preserve Git's parent relationships.

## Rust 0.50.14 - 2026-11-18

- Name the installed Windows app and shortcuts Shovel. Ship `Shovel.exe` in `Shovel-Setup.exe` and retain the existing signed update package and profile identities.

## Rust 0.50.13 - 2026-11-18

- Grey history commits ahead of the checked-out branch's configured upstream. Show green local branch badges and red remote branch badges, and hide HEAD aliases from the workspace.

## Rust 0.50.12 - 2026-11-18

- Add optional per-repository colored tab markers. A hollow circle means the working tree is clean, and a filled circle means it has local changes.

## Rust 0.50.11 - 2026-11-18

- Apply refreshed history and sidebar data after open menus close. Preserve the visible commit's scroll position across ordinary refreshes when it remains on the page.

## Rust 0.50.10 - 2026-11-18

- Reduce the top toolbar height and tighten spacing in the repository status card.

## Rust 0.50.9 - 2026-11-18

- Check the selected Rust update channel at startup and every hour by default. Show an Update Shovel button by the repository status when a verified newer release is available.

## Rust 0.50.8 - 2026-11-18

- Show the current Git phase and pending requests in the central repository status. Add activity spinners there, beside the lower Commit action, and in the Changed files controls while comparisons load.

## Rust 0.50.7 - 2026-11-18

- Choose a toolbar Commit window or a sidebar Local Changes view in File → Settings → Appearance. The sidebar switches between History and the in-workspace commit editor.

## Rust 0.50.6 - 2026-11-18

- Show changed files in the lower History pane as a left-aligned Change and Path table. Select a row to open its diff.

## Rust 0.50.5 - 2026-11-18

- Show Pull and Push badges only for positive commit counts.
- Open the Git output window automatically for failed foreground operations. Keep successful commands and background fetch activity in Activity.
- Add an Open In... destination icon and a separate Console icon that opens Git Bash in the selected repository.

## Rust 0.50.4 - 2026-11-18

- Draw a stash directly above its base on the same graph lane when no branch line already leads into that base.

## Rust 0.50.3 Stable promotion - 2026-11-18

- Owner-approved promotion of the existing 0.50.3 application to the independent Rust Stable channel; application files and Cargo version remain unchanged.
- Use a Stable installer/package, signed Stable feed and normal GitHub release while preserving Rust Beta, C# feeds and GitHub Latest.
- Fresh 0.50.3 profiles still default to Beta; select Rust Stable in update settings. Full parity and installed-update acceptance remain open.


## Rust 0.50.3 - 2026-11-16

- Make Rust the application on master; remove C# application/projects and obsolete delivery tooling, preserving history and old release feeds.
- Move application assets into the Rust tree and make release scripts support master.
- Build native Windows payloads and signed Rust Beta installers in GitHub Actions; publish new versions without changing C# feeds or GitHub Latest.
- Full functional parity, native UI acceptance, final installed update and paired benchmarks remain unfinished.

## Rust 0.50.2 - 2026-11-10 (local test build; not published)

- Recheck accepted work before final exit and preserve failed-save choices. Drain automatic fetch, repository reads, draft restoration, conflict previews and updater workers before closing.
- Track startup, checkout review, repository chooser and tool/submodule handoff workers through completion; keep accepted commands and required writes alive.
- Bound update HTTP phases without a 30-second whole-download cap.
- Local Windows x64 payload only. Full parity, native UI acceptance, final installed update and paired benchmarks remain unfinished.

## Rust 0.50.1 - 2026-11-10 (local test build; not published)

- Match shared Ctrl+Plus/Minus scaling to the C# 70-200% range and 10% steps, ignore held-key repeats, and persist changes without exiting. Keep newer shortcuts when an earlier settings save completes.
- Include repository-ordered Fetch/New Branch dialogs and submissions, native process ownership fixes, Unicode-compatible C#/Rust mutex names, and stale repository replacement rejection since the previous local build.
- Local Windows x64 payload only; full parity, native UI acceptance, final update verification and paired performance measurements remain open. No publication, installation or merge.

## Rust 0.50.0 - 2026-11-10 (local test build; not published)

- Add selected submodule Open, indexed Update and URL Sync, with literal paths, parent/child ownership, native dirty-file protection and retained Activity output.
- Show cached sidebar branch tracking details; preserve preparation-free menus and refresh affected child tabs.
- Include the preceding Rust work: native init/clone and remote management, reviewed branch activation, exact-target sidebar Fetch, ahead/behind badges, startup/history retention and repository-owned queues/draft recovery.
- Include the source fix for non-default UI-scale geometry; native Windows geometry and wider visual/authentication acceptance remain open.
- Produce a local-only Windows x64 test payload. No installer execution, GitHub publication, main merge or Rust Stable promotion. Full parity, final installed update and paired performance evidence remain in TODO.

## Rust 0.49.2 - 2026-11-07 (Rust Beta)

- Publish native Rust installers and signed updates through the existing GitHub credential integration.
- Verify artifact hashes, exact Rust release commits, anonymous downloads and C# release isolation during publication.
- Allow the owner-requested partial Rust Beta while keeping full parity and explicit approval mandatory for Rust Stable.
- Preserve the independent Rust application, settings and update identity. Remaining feature and verification gaps stay in TODO.

## Rust 0.49.1 - 2026-11-07 (installer preview, separate Rust branch)

- Accept the pinned Velopack packager’s NotesHTML field while rejecting duplicate/unknown metadata fields; retain signature and Rust channel verification.
- Align sidebar, Branch placement, file-list sizing and graph curves with the C# workspace.
- Locate tags and stashes across history pages; preserve tab dragging and pane layouts.
- Use remembered modal Fetch/Pull/Push options, categorized settings and cancellable automatic fetch.
- Add global Git identity editing, repository Open in tools, shared action history and Rust-only signed update discovery/download/restart.
- Add signed native Windows installer packaging without merging C# or changing its release feeds. Full parity and installed-update/performance acceptance remain pending.

## 0.47.0 - 2026-11-05 (Beta)

- Attach stashes directly above their bases without reordering ordinary history; share projection between paging and navigation.
- Confirm checkout with clean/dirty-aware Keep, Stash and reapply, or Discard tracked changes choices.
- Remember Fetch/Pull/Push options and apply them to the main buttons; keep explicit destinations branch-bound.
- Preserve sidebar expansion, window geometry and pane proportions; keep Commit resizable with close-only caption controls.
- Accept harmless index stat refreshes while retaining stale staged-content protection; compact empty-log notifications.
- Consolidate mutation locks, local action preparation and option capture; remove unused timer presentation and document state ownership.
- Replace the outdated packaged feature manual with a concise launch guide linked to current documentation.
- Keep Stable at 0.39.1.

## 0.46.0 - 2026-11-04 (Beta)

- Keep initialization and accepted Git operations alive when Commit opens or tabs/views close.
- Run independent work concurrently and queue conflicting actions with individual waiting-request removal.
- Close Commit on submission; retain staged files and drafts on failure, with explicit Reopen Commit actions.
- Stop only the affected repository's waiting queue after failure; preserve running work and other repositories.
- Protect started user commands from UI cancellation and timeout termination; keep automatic fetch preemption and guarded app exit.
- Keep Stable at 0.39.1.

## 0.45.0 — 2026-11-04 (Beta)

- Expand File → Settings into left-side Appearance, History & avatars, Diff, Git, Git identity, Tools and Updates categories.
- Expose saved scale, theme, diff layout/font, ref placement, auto-fetch cadence, tool paths and update channel/check preferences; apply them after a successful profile save.
- Read and edit global Git name/email through explicit native Git actions, with stale-value and lock failure handling.
- Show author pictures in history and Commit details, loading GitHub/configured Gitea or Gravatar images in the background with colored initials as fallback.
- Bound avatar requests and session caching; keep unauthenticated/private-profile failures quiet and offer initials-only mode.
- Keep Stable at 0.39.1.

## 0.44.0 — 2026-11-04 (Beta)

- Keep automatic fetch failures in Background activity without attention dialogs or taskbar flashing; change the default interval to one minute.
- Separate fetch ownership from working-copy actions, keep repository opening and context actions usable, and give submitted user work priority over automatic fetch.
- Add historical-file context actions, including restore from HEAD/selected commit/parent, external diff, history/blame, tree browsing, LFS rules, export and path copying.
- Add a saved Left/Right placement preference for history branch/tag labels, default Left.
- Show executed command transcripts and output in activity details, including quiet operations.
- Keep Stable at 0.39.1.

## 0.43.0 — 2026-11-04 (Beta)

- Configure an application-wide external merge tool in File → Settings: Git default, a named tool with an optional executable path, or a custom Git-shell command.
- Save the tool alongside auto-fetch preferences and select it from the existing inline conflict pane; explicit named choices still override the saved preference for that file.
- Run through native Git mergetool with per-invocation overrides, preserving repository/global configuration, stale-state checks, cancellation and actual staged-result verification.
- Keep Stable at 0.39.1.

## 0.42.0 — 2026-11-04 (Beta)

- Add File → Settings with saved automatic-fetch enablement and a 10–3600-second interval; existing profiles retain the enabled/10-second default.
- Apply scheduling changes immediately; disabling requests background-only cancellation without delaying Settings closure or cancelling manual work.
- Serialize preference writes so older saves cannot overwrite new choices; retain form edits and errors when saving fails.
- Keep Stable at 0.39.1.

## 0.41.0 — 2026-11-03 (Beta)

- Accept unique queued toolbar actions during repository reads/operations, retaining the original repository and validating targets when execution starts. Keep other repositories usable.
- Remove elapsed counters, timing badges, fading animations and the indeterminate operation strip; retain static status, cancellation and activity logs.
- Continue tab drag feedback across history, preview a full tab slot and move neighbors to their prospective positions.
- Show inspected branch context before history completes; verify viewport virtualization without reducing the 200-row page.
- Stable remains 0.39.1.

## 0.40.0 — 2026-11-02 (Beta)

- Keep Git operations/results with their repository while switching tabs and working in other repositories; retain common-directory serialization.
- Add ten-second auto-fetch with bounded concurrency, noninteractive credentials, background activity and cancellation on close; F5 refreshes repository/local changes.
- Close Commit after success and support Ctrl+Enter throughout the editor. Retain failed commits and notify separately when an optional push fails.
- Show attention dialogs with latest logs for failures/conflicts/warnings and flash the inactive taskbar button; deduplicate repeated background failures.
- Preserve pane ratios while resizing, keep stash connections from taking ordinary commit lanes, and put conflict-banner buttons on the left.
- Stable remains 0.39.1.

## 0.39.1 — 2026-11-01 (Beta and owner-approved Stable)

- Correct the optional history benchmark for retained rows and automatic Commit comparisons; report synchronous history publication separately from comparison drain.
- Validate merge-heavy, large and million-commit histories plus headless idle behavior without reducing All Commits scope or the 200-row page size.
- Publish the current application behavior to Stable with owner approval, including side-by-side diffs, saved UI scale and approval before update download/restart.


## 0.39.0 — 2026-11-01 (Beta)

- Add side-by-side text comparisons to history and Local Changes, preserving whole-hunk staging and binary cards.
- Check immediately on opening updates or changing channel. One approval downloads the offered version and requests a verified, guarded restart; launch checks no longer download or restart automatically.
- Stable remains 0.35.0.


## 0.38.0 — 2026-11-01 (Beta)

- Combine author, commit message, refs, SHA and clickable files/diffs in the bottom Commit view. Keep Changes as the comparison-only view; remove Repository and Diagnostics tabs and preserve the selected tab across commit selections.
- Add persisted Ctrl+Plus/Minus application scaling in 10% steps (70–200%), with no settings control yet. Remove Preview from the title.
- Use the installed shortcut app identity at process startup; add a translucent repository drag label, insertion marker and neighboring-tab feedback.


## 0.37.0 — 2026-11-01 (Beta)

- Edit BOM-marked UTF-16 little- and big-endian conflicts inside the existing Commit diff pane, alongside UTF-8. Show the result encoding and preserve it when saving and staging.
- Reject malformed Unicode and enforce the 512 KiB limit in the saved encoding, including its BOM. Preserve a leading content U+FEFF instead of stripping it with the transport BOM.
- Keep whole-file side controls, native filters/hooks, stale-file checks and retained edits after failed staging. Unsupported encodings and custom transformations retain side/tool resolution. Stable remains 0.35.0.

## 0.36.1 — 2026-11-01

- Reduce standard LFS discard validation and refresh costs by checking selected index pointers and actual hydrated bytes directly; retain native status for unsupported or uncertain selections.
- Share exact-content verification between status and discard, with no mutable file-hash cache. Preserve native checkout, LFS filters, index hooks, partial staging, cancellation and honest failure reporting.
- Add focused concurrent-edit, stale-index, fallback and missing-LFS-object checks. Publish Beta only; Stable remains 0.35.0.

## 0.36.0 — 2026-11-01 (Beta)

- Select an external merge tool inside the Commit conflict pane from configured names and common Git-supported tools; keep the configured default when preferred.
- Launch the chosen tool through native Git, preserve configuration and validate stale state and actual resolution/staging. Selection itself performs no Git work.
- Publish only to Beta; Stable remains 0.35.0 pending owner approval.

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
