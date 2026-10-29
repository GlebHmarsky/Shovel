# Task 1 handoff — local branches and upstream tracking

2026-10-29. Base: `146b0f3b278c12dc53e732e50d51e93e6d12f685`. Branch: `codex/task-1-branch-management`. This is implementation source with written, unexecuted checks; no build, test or runtime verification is claimed.

## Implementation

The existing Branch arrow menu now opens Delete local branch, Set upstream and Unset upstream options immediately from the selected tab's cached refs. The dedicated fixed-size dialog uses shared theme resources/native title-bar styling and resource-backed labels/help. Delete shows repository, full local ref, full tip and current upstream, explains reflog/config removal and normal Git merged protection, and defaults Enter to Cancel. Selecting the current branch disables deletion with a reason. Other-worktree occupancy is validated after submission rather than adding opening-time work. Closing options executes nothing and preserves the previous operation result.

`Core/Git/BranchManagement.cs` owns `BranchManagementOptions`, post-submission preparation and execution. Preparation checks identity, HEAD/refs and effective configuration without resolving remote URLs or contacting a remote. Execution acquires the existing common-directory mutation mutex, rechecks identity, refs, HEAD, sequence state and configuration, validates concrete target refs, and reads `worktree list --porcelain -z` before deletion. A final validation pass precedes the structured write:

- Delete: `branch --delete -- <local-name>`; no force, remote deletion or implicit checkout. Git decides merged status against the configured upstream, or HEAD when no upstream is configured.
- Set: `branch --set-upstream-to=<full-remote-tracking-ref> -- <local-name>`; Git writes the actual remote/merge mapping.
- Unset: `branch --unset-upstream -- <local-name>`; this also supports a configured upstream whose ref has disappeared. If no tracking is configured, Git's nonzero result remains an error.

`EnterMutationGate` extracts the original acquisition logic into the new Core module; the existing general action path uses that same helper. Other action commands and their gate name are unchanged. Mutex acquisition/release stays on one worker while async operations finish. Writes retain configured Git hooks, Git locking, a two-minute process timeout, bounded/redacted in-memory output and owned cancellation.

`ShellBranchManagement.cs` uses the existing `GitActionViewModel` operation lifetime for checking/progress/duration/cancellation/results. Options close before validation starts; completed process results retain the reviewed branch/tip and requested upstream beside the original Git output. Every submitted attempt, including stale-target failure and cancellation, invalidates related worktree caches and refreshes selected history, labels and local ahead/behind before reopening navigation; failed refresh is distinct from the command outcome. A cancelled command may already have changed a ref and never triggers retry, rollback or lock cleanup.

Shared edits are limited to the named menu/interaction/shell wiring, additive `BranchManage*` resource entries and the small Core enum/gate change. The user additionally authorized the two title/explanation lookup lines in `GitActionViewModel.cs`, so retained branch-management operations display their own localized titles. No draft, commit, history algorithm, settings schema, release or benchmark module was edited.

## Written checks and registration

`tests/GitGui.FoundationChecks/BranchManagementChecks.cs` contains 13 named checks. They cover merged/unmerged deletion and Git's upstream-based merged test; current and linked-worktree occupancy in both directions; current/noncurrent tracking and real config/count metadata; missing refs/upstream and symbolic aliases; changed tips/config/identity; active sequences and retained ref/config locks; shared-gate exclusion/cancellation; timeout/cancelled process results including a mutation completed before cancellation reporting; cached dialog construction/default button/frozen targets; and bound shell actions, refresh, output, cancellation and failed reconciliation. Real mutations are confined to disposable fixtures under the existing `_root`; remotes are unavailable local paths, with tracking refs seeded locally. Held/interrupted-runner cases are controlled simulations, separate from the real Git mutation cases.

SDK default source/XAML inclusion means no project-file registration is needed. Do not run this method outside the existing pumped dispatcher/isolated Git-config harness. The worker intentionally leaves shared test dispatch untouched. The integrator should make these exact additive changes:

1. In `ViewModelChecks.cs`, inside `RunViewModelChecks`'s local `RunAsync`, add `await RunBranchManagementChecksAsync();` to the normal sequence immediately after `await RunInitializationChecksAsync();`. This registers all 13 checks in the full suite.
2. For focused dispatch, append `bool branchManagementOnly = false` to `RunViewModelChecks`'s parameter list. Include `|| branchManagementOnly` in its `focused` expression. At the beginning of its local `RunAsync`, add `if (branchManagementOnly) { await RunBranchManagementChecksAsync(); return; }`.
3. In `Program.cs`, after `_root` is initially assigned and before `Directory.CreateDirectory(_root)`, add `if (args.FirstOrDefault() == "--check-branches") _root = Path.Combine(Path.GetFullPath("artifacts/branch-management-checks"), Path.GetFileName(_root));`.
4. Immediately after `Directory.CreateDirectory(_root)`, register the focused mode using the same existing pattern:

```csharp
if (args.FirstOrDefault() == "--check-branches")
{
    var app = new HeadlessApplication(); app.InitializeComponent();
    RunViewModelChecks(branchManagementOnly: true);
    Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}");
    return _failed == 0 ? 0 : 1;
}
```

After integration restore/build, the focused command is `dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build -- --check-branches`. Run the full foundation suite too, particularly existing branch/network/recovery/shared-gate regressions. Commands and release workflow remain owned by README; none were executed on this worker branch.

## Static review and limits

Reviewed command construction, cached opening paths, immutable submitted refs, common-mutex ownership, cancellation/reconciliation, resource and binding names, SDK item inclusion, fixture isolation and scope of changes. XAML/resource XML syntax parsing and duplicate-key inspection succeeded; `git diff --check` was clean. These are static observations, not C# compilation, WPF binding/layout execution or passing tests.

External Git clients do not participate in Shovel's mutex. Ref/config/occupancy checks are observations; a client can race the final read and write or perform an ABA change. Git's normal ref/config locks and branch protections remain authoritative; this feature does not promise an atomic expected-tip transaction against arbitrary external clients. Deletion can partially finish before a later config/hook/inspection failure, and cancellation can arrive after a write. Reinspection reports what exists without automatic repair. Configuration present at submission is used, then stamped/rechecked; options do not read config just to open.

Cached refs do not encode symbolic-ref status. The conventional remote `/HEAD` alias is excluded from choices; Core rejects other symbolic aliases with localized help after submission. Missing tracking configuration produces a normal Git error on Unset. Standard remote fetch mappings must let Git resolve the selected remote-tracking ref; unusual or ambiguous mappings may be rejected by Git. Existing supported-repository identity restrictions still apply. No remote deletion, force deletion, fetch, checkout or dependency was added.

Explicitly unverified: compilation/nullability/analyzers; execution of all 13 new checks and existing regressions; actual Git branch/config/hook/lock/cancellation behavior; headless WPF bindings/default-button/layout; visible dialog opening, keyboard/focus, high contrast, screen reader, text expansion/RTL/DPI and Windows 10. No timing or performance measurement was made. The test source does not certify these behaviors until integration runs it.

Integration still owns sequential application/review of the worker commits, test-dispatch registration, concurrency/startup regression verification, owning product/architecture/roadmap/TODO updates, one capability version/changelog entry, restore/build/checks and matching ignored preview. No worker merge, version increment, changelog edit, restore, build, test execution, app launch, dependency installation, preview or push occurred.
