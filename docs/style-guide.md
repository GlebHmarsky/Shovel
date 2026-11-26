# Shovel UI style guide

Use the Fetch, Pull and Push dialogs as the starting point for Shovel forms. The [shared theme](theme.md) explains the color roles. The Rust implementations are in the [workspace style](../rust/desktop/src/workspace_style.rs) and [native window placement](../rust/desktop/src/native_windows.rs) modules.

## Ownership and focus

- Open forms, approvals and destructive reviews as a modal in the Shovel viewport that opened them. Center in its visible client area. A review from the standalone Commit window belongs inside that window.
- Keep the owner visible behind the common dimming layer. Block its controls, tabs, shortcuts, caption actions and focus traversal. Close stale owner menus on entry. Tab and Shift+Tab navigate the dialog, and Enter activates the focused control. Restore owner focus when the dialog closes.
- Put the close control in the upper-right corner with a visible resting target, a hover state, a keyboard focus outline and a localized accessible name. Use Escape for cancellation when no dropdown is open. Keep Cancel and close disabled whenever the operation requires the dialog to stay open.
- Preserve the close-only, resizable native Commit caption. Center separate Shovel viewports over their launcher using its monitor work area and Windows DPI. Keep an inspector's saved size when it fits. Let Windows draw and operate system file and folder choosers with the correct Shovel owner.
- Activity, Help, File History, Blame and the Stashes inspector support modeless reading. A confirmation opened inside an inspector blocks interaction until dismissed. Large inspectors can scroll and resize to suit their content.

## Layout and actions

- Put the task name on one heading line. Leave room between the title and the close control. Fit short bodies to their content, and cap long bodies at the available height. Scroll affected paths, output and long forms so the action row stays visible.
- Align related fields. Use shared spacing, text, input and panel colors. Show a useful empty or unavailable message and leave only applicable actions enabled.
- Keep the action row horizontal at the lower right. Use a concrete verb and target for a high-impact action. Pair semantic green, amber and red with text and a visible selected or focused state. Respect Windows high contrast and preserve legibility in Light and Dark modes.
- Name the branch, remote, repository or paths the action affects. Include a short consequence if the scope fails to convey what will change. Keep log output in an expandable or bounded reading area.

## Commit information

- Use the subject, an eight-character SHA, and the shared author picture or initials with the author name. Add the date when it helps distinguish commits. Put the full SHA on hover and keep it in the captured Git request. Leave the body for commit details.
- Default Reset Branch to Here to Mixed. Let the user compare Soft, Mixed and Hard before choosing the named branch action. Describe what Hard discards and any obstructing untracked files Git may remove. Undo Last Commit uses Soft and reviews the first parent.
- Build reviews from the actual captured repository and immutable target. Read any missing metadata off the UI thread. Revalidate through the regular Git mutation path on submission.

Review compact and normal layouts with both palettes and keyboard and pointer input. Keep physical mixed-DPI, screen-reader and installed checks in the [acceptance checklist](../TODO.md).
