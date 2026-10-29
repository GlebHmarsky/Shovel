# Shared visual theme

Updated 2026-10-30. **Confirmed direction:** restrained Fork-inspired dark surfaces, with live Dark/Light selection added in 0.24.0. Further polish stays P2 in [TODO](../TODO.md). This document owns palette/state conventions; [product](product.md) owns workflow and [roadmap](roadmap.md) records verification. No Fork artwork, branding or proprietary style resources are used.

## Palette and hierarchy (0.12.1)

The screenshot suggests dark chrome and inactive tabs, a charcoal history surface, lighter sidebar/details, subtle separators and an orange active lower tab. Values below are chosen approximations, not pixel-exact Fork measurements. `Resources/DarkPalette.xaml` is the implementation source; views use semantic `DynamicResource` keys instead of inline colors.

| Role (`Theme.` prefix) | Color | Use |
| --- | --- | --- |
| Chrome / TabStrip | #1B1B1B / #171717 | Menu/toolbar; inactive repository tabs |
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
| AuthorPeer | #383622 | Muted yellow background for same-author peers; ordinary text, never the selected row |
| DiffAddedBackground / DiffAddedText | #213A2B / #B8E6C4 | Added lines, accompanied by + and new line number |
| DiffRemovedBackground / DiffRemovedText | #46252A / #FFB8BD | Removed lines, accompanied by − and old line number |
| FileModified / FileAdded / FileDeleted | #E5C07B / #80C58B / #FF8B93 | M / + / − file markers in committed and working-copy lists; selected text/system high contrast overrides colors |

## Light counterpart (0.24.0)

`LightPalette.xaml` uses the same semantic keys: Chrome #F3F3F3, tabs #E8E8E8, sidebar #ECECEC, white Canvas/Input, Panel #F7F7F7, Raised #E4E4E4, Border #B0B0B0, Text/HistoryText #202020 and Muted #555555. Selection/hover use blue fills with white text; Accent #A0391D, Focus #245F9C. AuthorPeer is pale yellow #FFF3BF with ordinary dark text. Diff/marker colors use darker foregrounds over pale fills. All shared views consume these resources; switching adds no scans/reloads. Dark remains the initial/default appearance.

History graph strokes use compact 16-DIP lanes, straight runs and rounded joins; merges are hollow circles with a center marker; stashes are hollow rounded squares with one base connection. Gold/red/green/blue/purple strokes have darker light-theme counterparts. Selected rows retain graph colors; OS high contrast uses system text. Since 0.26.0, Core keeps fixed branch tracks and projects ordinary stash entries to their base; see the caption/history architecture.

## Borders and operation feedback (0.25.0)

Use one-DIP shared Border separators; retain open, headerless history rows rather than boxing every cell. Buttons/navigation use 4-DIP radii; repository/lower tabs round only the top corners (5/4 DIP); popup surfaces use 5 DIP with 3-DIP menu-item corners. The center context panel uses a 6-DIP radius and quiet outline. Focus, hover, selected and disabled states continue to use existing semantic roles and native input behavior.

The duration capsule uses an 11-DIP radius, fixed width and tabular digits. New `Operation{Running,Success,Warning,Failed}{Background,Text}` roles pair blue, green, amber and red surfaces with high-contrast text in both palettes. Cancellation uses Raised/Muted. Status text and symbols distinguish states without color. High contrast maps these roles to system window/text brushes. The capsule fades in for a new attempt and out after completion; its fade does not change layout. See [behavior](product.md#operation-feedback-0250); no added Git work or perpetual idle animation.

## State rules

- Main caption/menu strip is 32 DIP with 23-DIP caption glyphs in 44×32 targets, sharing Chrome/Text with localized caption controls and existing focus/hover behavior. WindowChrome supplies native move/resize hit testing; menus/buttons remain interactive. Dialog captions are unchanged.
- History only: fixed branch tracks and up to 10-DIP curved joins replace the prior small double bends (0.26.0). Preserve graph colors/merge markers/high contrast and compact 16-DIP lane spacing. Folding a stash changes its graph to one base connection, while details retain its true parents.

- Distinguish surfaces by role, not arbitrary shades per window. Main history begins at the repository tabs; theme changes add no toolbar, padding band or data preparation.
- Repository tabs: quiet inactive surface, lighter selected tab, hover feedback and a keyboard focus outline. Lower tabs: orange text plus underline; selection remains identifiable without color alone.
- Buttons: raised or flat toolbar base, hover/pressed feedback, visible keyboard focus/default action outline. Disabled text and icon color override interaction text; retain actual command availability and its localized reason. No hover animations or theme-driven background work.
- Rows: blue active selection, theme-specific inactive selection, white selected text. Same-author yellow is a background limited to unselected peers, with unchanged ordinary text; graph/ref semantics and accessible relationships remain unchanged. Selection and keyboard behavior belong to native WPF controls.
- Diff rows follow the same selection rules. Unselected additions/removals use shared green/red roles; hunk headings use Raised/Focus. Markers, old/new line numbers and localized accessible descriptions remain meaningful when high contrast removes the colors. No per-file ad hoc palette.
- Binary comparisons reuse Raised/Border cards, RemovedText for Old and AddedText for New; LFS uses the existing selection badge. Extension and size remain normal text with accessible side/size meanings. No per-extension color scheme or bottom loading animation.
- Editable fields, lists, menus, checkboxes/radio buttons and scrollbars share the palette. Use template parts required by WPF; do not replace commands, editable text, focus/selection logic or virtualization with visual-only imitations.
- Normal/secondary text, selected text and accent text must meet 4.5:1 against their intended dark surfaces. Disabled text is intentionally lower contrast and never the only way to discover why an action is unavailable. Focus uses an outline, lower-tab selection an underline, checked options a mark.
- Windows high contrast overrides semantic colors with system window/text/selection/disabled brushes and suppresses peer color. Never change OS settings. OS keyboard/screen-reader/high-contrast/DPI behavior still needs authorized desktop validation.

## Scope and maintenance

Shared styles live in `Resources/AppStyles.xaml`; `Services/AppTheme.cs` applies the selected palette at startup, on Appearance selection and system high-contrast changes, and unsubscribes at exit. Schema 5 stores the selection; older profiles default to Dark. No new dependency, network request or Git-path change. Headless tests use the same resources with a plain test Application.

Since 0.16.1, Shovel-owned window title bars use native DWM styling; 0.24.0 updates that styling on palette changes. Windows 11 caption/background/text/border colors follow Chrome, Text/Muted and Border; high contrast removes these overrides. Dialog caption buttons remain native. Since 0.26.0 the main window uses an integrated WindowChrome caption with SystemCommands buttons; move/resize/system-menu hit testing remains delegated to WPF/Windows. Windows 10 receives a best-effort light/dark-mode request; unsupported attributes retain native defaults. Windows file/folder pickers and MessageBox dialogs remain OS-owned. Wider visual/accessibility polish stays deferred. Do not claim pixel parity or desktop certification from an offscreen render.

Implementation uses stable [WPF styles/templates](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/controls/control-styles-and-templates). The .NET 10 [Window.ThemeMode API](https://learn.microsoft.com/en-us/dotnet/api/system.windows.window.thememode?view=windowsdesktop-10.0) is still marked experimental in official documentation checked 2026-10-28; this pass avoids adopting it or changing control metrics through Fluent theme migration.

## Conflict and modal surfaces (0.30.0)

Merge/rebase banners sit above history using Theme.SequenceBackground/SequenceBorder (orange in Dark/Light, system window/text in high contrast). Conflict panes use shared text/input styles and native selection/scrolling. The repository ListBox has an explicit semantic border/scroll template, so owner disabling for Commit cannot paint the strip white. Drag feedback uses WPF's native loop; desktop drag/DPI and native modal paint still need interactive validation.
