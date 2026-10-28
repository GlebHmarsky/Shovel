# Shared visual theme

Updated 2026-10-28. **Confirmed direction:** a restrained dark appearance inspired by the supplied Fork screenshot, with a first pass now and further polish at P2 in [TODO](../TODO.md). This document owns palette/state conventions; [product](product.md) owns workflow and [roadmap](roadmap.md) records verification. No Fork artwork, branding or proprietary style resources are used.

## Palette and hierarchy (0.12.1)

The screenshot suggests dark chrome and inactive tabs, a charcoal history surface, lighter sidebar/details, subtle separators and an orange active lower tab. Values below are chosen approximations, not pixel-exact Fork measurements. `Resources/DarkPalette.xaml` is the implementation source; views use semantic `DynamicResource` keys instead of inline colors.

| Role (`Theme.` prefix) | Color | Use |
| --- | --- | --- |
| Chrome / TabStrip | #1B1B1B / #171717 | Menu/toolbar/footer; inactive repository tabs |
| Canvas | #222222 | History and file-list surface |
| Sidebar / Panel | #2B2B2B / #323232 | Navigation/selected repo tab/lower tab headers; details content |
| Raised / Input | #303030 / #272727 | Repository context, buttons/popups; editable fields |
| Border | #484848 | Quiet structural separators, not the only focus cue |
| Text / HistoryText / Muted | #E4E4E4 / #FFFFFF / #B8B8B8 | Primary, history and secondary text |
| Disabled | #858585 | Unavailable text/icons; reason remains available through help |
| Hover / Pressed | #3B3B3B / #454545 | Pointer/action feedback |
| Selected / ActiveSelection | #4A4A4A / #2465AA | Inactive row/navigation selection; focused row selection |
| SelectionText | #FFFFFF | Text on selection/interaction fills |
| Accent / Focus | #F08060 / #7BB7F5 | Selected lower tab/default-action outline; keyboard focus |
| AuthorPeer | #FFF2A8 | Existing same-author relationship, never the selected row |
| DiffAddedBackground / DiffAddedText | #213A2B / #B8E6C4 | Added lines, accompanied by + and new line number |
| DiffRemovedBackground / DiffRemovedText | #46252A / #FFB8BD | Removed lines, accompanied by − and old line number |
| FileModified / FileAdded / FileDeleted | #E5C07B / #80C58B / #FF8B93 | M / + / − file markers in committed and working-copy lists; selected text/system high contrast overrides colors |

## State rules

- Distinguish surfaces by role, not arbitrary shades per window. Main history begins at the repository tabs; theme changes add no toolbar, padding band or data preparation.
- Repository tabs: quiet inactive surface, lighter selected tab, hover feedback and a keyboard focus outline. Lower tabs: orange text plus underline; selection remains identifiable without color alone.
- Buttons: raised or flat toolbar base, hover/pressed feedback, visible keyboard focus/default action outline. Disabled text and icon color override interaction text; retain actual command availability and its localized reason. No hover animations or theme-driven background work.
- Rows: blue active selection, gray inactive selection, white selected text. Same-author yellow remains limited to unselected peers; graph/ref semantics and accessible relationships remain unchanged. Selection and keyboard behavior belong to native WPF controls.
- Diff rows follow the same selection rules. Unselected additions/removals use shared green/red roles; hunk headings use Raised/Focus. Markers, old/new line numbers and localized accessible descriptions remain meaningful when high contrast removes the colors. No per-file ad hoc palette.
- Editable fields, lists, menus, checkboxes/radio buttons and scrollbars share the palette. Use template parts required by WPF; do not replace commands, editable text, focus/selection logic or virtualization with visual-only imitations.
- Normal/secondary text, selected text and accent text must meet 4.5:1 against their intended dark surfaces. Disabled text is intentionally lower contrast and never the only way to discover why an action is unavailable. Focus uses an outline, lower-tab selection an underline, checked options a mark.
- Windows high contrast overrides semantic colors with system window/text/selection/disabled brushes and suppresses peer color. Never change OS settings. OS keyboard/screen-reader/high-contrast/DPI behavior still needs authorized desktop validation.

## Scope and maintenance

Shared styles live in `Resources/AppStyles.xaml`; `Services/AppTheme.cs` applies palette overrides at startup and on system high-contrast changes, and unsubscribes at exit. No dependency, settings schema change, theme selector, network request or Git-path change is introduced. Headless tests use the same resources with a plain test Application.

Native window title bars, Windows file/folder pickers and MessageBox dialogs remain OS-owned. Light mode, optional theme selection, native chrome integration and wider visual/accessibility polish are deferred; keep their tasks in TODO, not a second checklist here. Do not claim pixel parity or desktop certification from an offscreen render.

Implementation uses stable [WPF styles/templates](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/controls/control-styles-and-templates). The .NET 10 [Window.ThemeMode API](https://learn.microsoft.com/en-us/dotnet/api/system.windows.window.thememode?view=windowsdesktop-10.0) is still marked experimental in official documentation checked 2026-10-28; this pass avoids adopting it or changing control metrics through Fluent theme migration.
