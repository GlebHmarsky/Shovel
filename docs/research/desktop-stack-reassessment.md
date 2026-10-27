# Desktop stack reassessment

Historical research **2026-10-23**, no builds/installs/benchmarks in this comparison. User subsequently confirmed **C# + WPF**; implementation chose .NET 10 LTS. [Architecture](../architecture.md)/[roadmap](../roadmap.md) own current decisions/evidence. Alternatives and earlier distribution advice are historical; confirmed startup updater policy takes precedence.

## Decision basis

AI maintains code, user familiarity with TypeScript/C++ is not the criterion; TypeScript excluded. App needs no AI API/uploads/model. No evidence that AI writes one language more correctly: evaluate APIs/compiler diagnostics/tests/docs/dependency stability/automation/custom infrastructure. Every stack needs real Git integration tests.

C#/WPF/supported .NET LTS/Git for Windows CLI fits Windows-only dense desktop best. Qt Widgets is strongest mature native alternative; Slint strongest Rust candidate reviewed, with more toolkit validation. No mixed-language infrastructure without measured need.

| Candidate | Benefit | Cost/judgment at research date |
| --- | --- | --- |
| C#/WPF | Mature controls/commands/resources/automation/layout/async | Managed footprint, runtime XAML bindings, custom graph/diff; selected |
| C++/Qt Widgets | Model/view/i18n/native build, footprint potential | Lifetime/ABI/toolchain/deployment/licenses; runner-up if measured footprint dominates |
| Rust/Slint | Types/ownership/native declarative UI/AI inspection | Prove editing/accessibility/RTL/virtualization/Windows interaction; reevaluate only if WPF misses needs |
| Rust/Iced | Typed state/messages/async | Project describes experimental; not selected |
| Go/Fyne | Small language/simple logic/no browser | Custom rendering; dense UI/text selection/accessibility/RTL need proof |
| Go/Wails | Go/desktop integration | Web frontend/WebView2 still required; insufficient simplification |
| C#/Avalonia | .NET/cross-platform/platform accessibility | Extra toolkit/rendering for Windows-only; consider if cross-platform needed |

Microsoft recommends WinUI 3 for new Windows apps; WPF selection is specific to established dense desktop patterns, not inability of WinUI/Fluent to implement them.

## Technical findings and performance limits

- WPF UI virtualization is not data virtualization: incremental Git reads/bounded caches required even with few realized rows. Resources/FlowDirection support localization; modern .NET cannot reuse old Framework LocBaml unchanged.
- Qt has mature model/view/i18n; module licenses vary.
- Slint 1.17 (June 2026) added tray, single-window drag/drop, tooltips and optional MCP tree/input/screenshots. Useful but recent desktop maturation; old “no tray” claim obsolete.
- Iced marked experimental. Fyne setup needs Go **and C compiler**. Wails needs web frontend/WebView2. Avalonia uses Skia/Windows UI Automation; no basis to call it inherently inaccessible.
- Qt/Slint may reduce baseline runtime/startup costs; backend/packaging affect outcome. WPF avoids browser tree but carries .NET. Wails binary size excludes total WebView2 cost; Fyne also has rendering dependencies.
- Idle CPU depends on timers/polling/repaints, not language. Same-fixture measurements must include app/children memory, cold/warm startup, idle CPU, package size and UI stalls. Retain proposed budgets until measured; no invented language-specific MiB figures.

## Maintenance principles

Small explicit UI/workflow/Git/Windows/test/resource modules; typed snapshots/shared commands; no generic plugin/sprawling abstraction. Nullable/analyzers/XAML-binding diagnostics; stable AutomationIds/peers, keyboard/screenshot checks. Disposable real-Git semantics for index/worktree/refs/cancel/recovery/races. Pinned tooling/reproducible commands/concise decisions/evidence. Debug automation only explicitly enabled, no default remote control service. Current policy requires functional features, not UI-only prototypes.

## Primary sources

Historical references; recheck versions/APIs when implementing.

| Topic | Official source |
| --- | --- |
| WPF virtualization | [Control performance](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/optimizing-performance-controls) |
| WPF localization | [Globalization/localization](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/wpf-globalization-and-localization-overview) |
| Automation | [Standard controls](https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-support-for-standard-controls) |
| WinUI | [WinUI 3](https://learn.microsoft.com/en-us/windows/apps/winui/winui3/) |
| Qt | [Model/view](https://doc.qt.io/qt-6/model-view-programming.html), [i18n](https://doc.qt.io/qt-6/internationalization.html), [licenses](https://doc.qt.io/QT-6/licensing.html) |
| Slint | [Desktop work](https://www.slint.dev/blog/making-slint-desktop-ready), [1.17/AI inspection](https://slint.dev/blog/slint-1.17-released), [terms](https://slint.dev/terms-and-conditions) |
| Other toolkits | [Iced](https://github.com/iced-rs/iced), [Fyne setup](https://docs.fyne.io/), [Wails](https://wails.io/docs/introduction/), [Avalonia Windows](https://docs.avaloniaui.net/docs/platform-specific-guides/windows/) |

At the time of this research only notes were added: no prototype/dependency/Git-init/benchmark work. Subsequent implementation approval/results belong to current project docs.
