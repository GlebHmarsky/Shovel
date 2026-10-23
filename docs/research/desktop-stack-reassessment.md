# Desktop stack reassessment

Historical research reference. [Architecture](../architecture.md) and [roadmap](../roadmap.md) own current decisions and evidence. The user subsequently confirmed C# + WPF on 2026-10-23; .NET 10 LTS was selected for the implemented foundation. Unselected alternatives below remain historical research. Newer updater requirements take precedence over earlier distribution suggestions.

Research date: 2026-10-23. This comparison itself contains no builds, installs or performance measurements; subsequent implementation evidence lives in the roadmap.

## Corrected decision basis

The user expects AI to maintain the implementation and does not expect to support it personally. Familiarity with TypeScript/C++ is not a selection criterion. TypeScript is removed from consideration. AI-maintained means the development/maintenance process; this does not add an AI service, repository uploads, or a model dependency to the shipped Git client.

There is no measured evidence here that an AI produces more correct applications in one language. Assess established APIs, compiler diagnostics, tests, documentation, dependency stability, UI automation, and how much custom infrastructure the project must own. Git semantic correctness still requires integration tests in any language.

## Recommendation

C# + WPF + supported .NET LTS + Git for Windows CLI remains the best overall fit for the specified Windows-only, dense desktop application. C++/Qt Widgets is the strongest mature native alternative. Rust/Slint is the strongest Rust candidate examined, but it carries more desktop-toolkit validation work. Do not introduce mixed C#/Rust or C#/C++ infrastructure without a measured bottleneck.

## Candidates

| Candidate | Strength | Main cost for this application | Judgment |
| --- | --- | --- | --- |
| C#/WPF | Long-established Windows controls, commands, resources, UI Automation, layout, asynchronous .NET APIs | Runtime footprint, XAML binding errors that may surface at runtime, custom graph/diff engineering | Recommended |
| C++/Qt Widgets | Mature model/view tables and trees, localization, native compilation, good footprint potential | Native lifetime/ABI/toolchain complexity, deployment and license requirements | Runner-up; preferable if measured footprint dominates |
| Rust/Slint | Ownership/type checking, native UI, declarative layout, AI inspection facility | More recent desktop facilities; must prove editing, accessibility, RTL, virtualization, and Windows interaction quality | Worth a focused evaluation only if WPF misses requirements |
| Rust/Iced | Typed state/message/update model and async support | Project currently describes itself as experimental | Not the foundation for this reliability-first application |
| Go/Fyne | Small language, straightforward application logic, no browser UI | Custom-rendered toolkit; exact dense desktop, text-selection, accessibility, and RTL requirements need proof | No clear advantage over WPF for this UI |
| Go/Wails | Go backend with desktop integration | Still a web frontend and WebView2 stack, even without TypeScript | Does not simplify the proposed architecture enough |
| C#/Avalonia | .NET, platform accessibility integration, cross-platform option | Additional rendering/toolkit dependency for a Windows-only target | Prefer if cross-platform delivery becomes a requirement |

WinUI 3 is Microsoft's recommended framework for new Windows applications. That recommendation is acknowledged, not ignored. WPF is selected here for a traditional data-dense desktop tool with established desktop interaction patterns, rather than a requirement for Fluent presentation or Windows App SDK features. This is an application-specific engineering judgment, not a claim that WinUI cannot implement the product.

## Verified details affecting the decision

- WPF provides UI virtualization, but Microsoft distinguishes it from data virtualization: binding a million objects can still consume memory even if few rows are drawn. We need incremental Git reads and bounded caches as well as virtualized controls.
- WPF supports localization and FlowDirection. Modern .NET cannot use the old .NET Framework LocBaml workflow unchanged. Use resource-backed messages and test long translations/RTL.
- Qt has mature model/view and internationalization APIs. Check each selected module's license; do not assume all Qt modules have the same license.
- Slint 1.17 (June 2026) added system tray support, single-window drag/drop, tooltips, and an optional MCP facility for UI tree inspection, input, and screenshots. These are useful for AI maintenance, but their recent introduction is also evidence of continuing desktop maturation. Do not repeat older claims that Slint has no tray support.
- Iced's repository explicitly labels it experimental.
- Fyne's getting-started documentation requires Go and a C compiler; do not promise a compiler-independent pure-Go build.
- Wails uses a web frontend and Windows WebView2; Go alone does not remove that dependency.
- Avalonia exposes Windows controls through UI Automation and renders using Skia. It is a legitimate option, not dismissed as inherently inaccessible.

## Performance expectations, not benchmarks

C++/Qt and Rust/Slint have strong potential for lower baseline memory/startup without a managed runtime. Actual rendering backends and packaging can change the result. WPF pays for .NET but avoids a browser process tree. Wails package size must not be confused with WebView2's total runtime cost. Fyne also has rendering dependencies.

Idle CPU primarily depends on scheduling, timers, polling, and repaint behavior; no language guarantees near-zero idle. Report app and child-process memory, cold/warm startup, idle CPU, installer size, and UI stalls using the same fixtures and machine. Retain the earlier proposed budgets until measured, rather than inventing language-specific MB numbers.

## AI maintenance requirements

- Small projects with explicit boundaries: UI, workflows, Git adapter, Windows process services, tests/resources.
- No generic plugin system or sprawling framework abstraction.
- Typed operations and immutable snapshots; one command definition reused by all UI entry points.
- Compiler nullable analysis/analyzers; XAML binding diagnostics included in UI test failures.
- Stable AutomationIds, automation peers for custom views, keyboard tests, and screenshot checks.
- Disposable Git fixture generation; semantic assertions for index/worktree/ref changes, cancellation, recovery, and external races.
- Pinned toolchain/packages, concise build instructions, architecture decisions, and per-phase verification reports.
- Debug/test UI automation only where explicitly enabled; no embedded remotely accessible control service in production by default.
- App remains a local deterministic Git client, with no AI API required to use it.

## Primary sources

- WPF controls/performance: https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/optimizing-performance-controls
- WPF localization: https://learn.microsoft.com/en-us/dotnet/desktop/wpf/advanced/wpf-globalization-and-localization-overview
- Windows UI Automation: https://learn.microsoft.com/en-us/dotnet/framework/ui-automation/ui-automation-support-for-standard-controls
- WinUI 3: https://learn.microsoft.com/en-us/windows/apps/winui/winui3/
- Qt model/view: https://doc.qt.io/qt-6/model-view-programming.html
- Qt localization: https://doc.qt.io/qt-6/internationalization.html
- Qt licenses: https://doc.qt.io/QT-6/licensing.html
- Slint desktop work: https://www.slint.dev/blog/making-slint-desktop-ready
- Slint 1.17 and AI inspection: https://slint.dev/blog/slint-1.17-released
- Slint license terms: https://slint.dev/terms-and-conditions
- Iced status: https://github.com/iced-rs/iced
- Fyne setup: https://docs.fyne.io/
- Wails architecture: https://wails.io/docs/introduction/
- Avalonia Windows integration: https://docs.avaloniaui.net/docs/platform-specific-guides/windows/

## Decision status

The user's latest request continues research, not implementation approval. Only these research notes were added. No dependencies, prototype, Git initialization, or benchmark run was performed.
