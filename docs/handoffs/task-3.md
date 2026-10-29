# Task 3 handoff — history performance harness

2026-10-29. Base `146b0f3`; branch `codex/task-3-history-benchmarks`. Implementation and static review only. **No restore, build, test execution, fixture generation, measurement, app launch or packaging was performed. No performance results are claimed.** Production code, existing benchmarks/dispatch, versions, release notes and owning project documents are unchanged.

## Implementation

New `tests/GitGui.FoundationChecks/HistoryPerformance*.cs` provide a standalone opt-in harness, streaming fixture generator, isolated runner, Windows process resource sampler and six small correctness checks. `scripts/Measure-History.ps1` invokes an already-built Release foundation executable; it refuses missing dispatch or a missing/older executable and never restores/builds.

Fixtures have deterministic messages, UTC timestamps, parent IDs and 1,200 additional refs spread over local branches, remote-tracking branches and tags. `merge-heavy`: 20,000 visible commits with repeated diamond merges; `large`: 100,000 linear commits; explicit `million`: 1,000,000 commits with diamond merges. All have empty trees, packed imported objects, packed refs and a reachable commit graph. This extends rather than replaces the existing loose-ref/no-graph F1e comparison. An extra unmerged remote-only tip, tag-only tip and detached HEAD exercise All Commits; one stash-only commit must be excluded. CLI count/merge/scope checks and sampled page hash/parent/topology comparisons are outside timing. Small tests walk the entire 803-commit fixture and compare graph edges/colors across all page boundaries.

Every run creates `%TEMP%\Shovel-HistoryPerformance-<guid>` with an ownership marker and empty config/template. Per-child environment removes inherited Git overrides/traces/config parameters and fixes system/global config, no-lazy-fetch, no prompts and no transports. No source/user repository is read, no network or global config changes occur. Only the exact marked temp tree is deleted, after rejecting path escapes/reparse entries and draining work; locked/undrained leftovers are reported for manual inspection. No arbitrary cleanup path is accepted. Normal/Ctrl+C failures retain a partial JSON report.

`artifacts/history-performance/<UTC>-<guid>/report.json` records OS/runtime/architecture/logical processors/processor identifier, GC-available memory (not installed RAM), filesystem/drive type, Git/App/harness versions, App assembly SHA-256, fixture spec, boundaries, per-stage times/outcomes/subprocess counts and sampled resources. Trace sub-stages overlap. Stages separate discovery, fixture creation/maintenance, core first page (inspection/read/row formatting), unchanged core refresh, sequential next page, last-offset probe, cancellation, actual shell first rows/drain, cached tab switching and shell refresh. Warm switches use two actual disposable repository tabs, preserve cached row identity and require zero Git calls. Shell first/refresh completion includes post-history divergence/count drain. No window is constructed/shown.

The sampler observes only this foundation host and descendants identified by Windows Toolhelp parent PID plus process start time, including observed children whose parent exits. Reports host/child private and working-set sampled peaks, simultaneous combined private peak, observed/live children, unavailable samples and CPU deltas. Default interval 50 ms; explicit five-minute idle uses 1,000 ms and requires zero periodic Git calls. CPU percent is CPU milliseconds / elapsed milliseconds / logical processors × 100. Sampler overhead is included, not subtracted.

Cancellation targets a real deep history request: start its delay when the production runner receives `log`, cancel the token, await the reader/owned Job drain and report cancellation-to-drain separately. If the read wins the race, report `completed-before-cancel`/`completed-after-cancel-race`; these are inconclusive cancellation samples. Production read timeouts/page size/scope remain unchanged. Imports have a 30-minute bound, maintenance/count commands five minutes, core read chains 45 seconds, shell waits 65 seconds plus shutdown drain 60 seconds, whole run two hours. Ctrl+C cancels owned work; cancellation is not rollback. Native filesystem calls/failed termination can outlast these cooperative limits; undrained fixtures remain intact.

## Exact integration registration

SDK default compile items include all new C# files; no project/dependency changes are needed. The following edits belong to the integrator, not this worker branch.

In `tests/GitGui.FoundationChecks/Program.cs`, insert this at the beginning of `Main`, **before its existing `_root` argument dispatch**:

```csharp
if (args.FirstOrDefault() == "--measure-history-performance")
    return HistoryPerformanceHarness.Run(args);
```

For the focused small tests, add this alongside the existing `_root` overrides, before `Directory.CreateDirectory(_root)`:

```csharp
if (args.FirstOrDefault() == "--check-history-performance")
    _root = Path.Combine(Path.GetFullPath("artifacts/history-performance-checks"), Path.GetFileName(_root));
```

Immediately after `Directory.CreateDirectory(_root)`, add:

```csharp
if (args.FirstOrDefault() == "--check-history-performance")
{
    RunHistoryPerformanceChecksAsync().GetAwaiter().GetResult();
    Console.WriteLine($"{_passed} passed; {_failed} failed. Fixtures: {_root}");
    return _failed == 0 ? 0 : 1;
}
```

Also register `await RunHistoryPerformanceChecksAsync();` at the end of `RunProcessChecksAsync()` for full-suite regression coverage. This registers only the small checks; never register `HistoryPerformanceHarness.Run` in the default suite. Checks cover strict opt-in/options, deterministic import/diamond parents, bounded streaming cancellation without generating a million commits, cleanup refusal, complete disposable CLI/reader/graph agreement and cancelled-reader reuse, and a sampled owned probe child/CPU normalization. They are written but **unexecuted**.

After review/registration and separately authorized restore/Release build, run from the integrated checkout:

```powershell
# Small checks only, existing build; no automatic restore/build.
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build --no-restore -- --check-history-performance

# Independent opt-in measurement commands; each creates and cleans its own fixtures.
powershell -NoProfile -File scripts/Measure-History.ps1 -Profile merge-heavy -Samples 3
powershell -NoProfile -File scripts/Measure-History.ps1 -Profile large -Samples 3
powershell -NoProfile -File scripts/Measure-History.ps1 -Profile million -Samples 3
powershell -NoProfile -File scripts/Measure-History.ps1 -Profile merge-heavy -Samples 3 -IdleFiveMinutes

# Equivalent direct dispatch (million/idle must still be explicit):
dotnet run --project tests/GitGui.FoundationChecks -c Release --no-build --no-restore -- --measure-history-performance merge-heavy --samples=3 --cancel-after-ms=50
```

Exit 0 means validations/completion/owned cleanup succeeded; 1 is failure/incomplete cleanup, 2 invalid options/platform, 130 outer cancellation. Read individual cancellation results even on exit 0. If cancellation races completion, rerun explicitly with `-CancelAfterMs 1` or the million profile. Retain failed/partial reports as failed evidence. Use the original README F1e commands for comparisons; do not compare mismatched fixture layouts or sum overlapping trace timings. The script's source timestamp check is a convenience, not proof of build provenance; rebuild integrated sources and retain the assembly hash.

## Limits and review

Static review covered ownership boundaries, protocol syntax/byte counts, parent/ref/count arithmetic, unchanged production page/scope, structured arguments/config isolation, cancellation/drain ordering, sampler identity/handle disposal, cleanup guards, dispatcher lifetime, source-only test registration and script parsing. Git/Win32 contracts were checked against [fast-import](https://git-scm.com/docs/git-fast-import.html), [Toolhelp snapshots](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot) and [PROCESSENTRY32W](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/ns-tlhelp32-processentry32w). No external dependency was added.

Everything remains compile/runtime-unverified, including native struct marshalling, WPF dispatcher/cache behavior, million-scale import, timeout/cancellation races, cleanup after real process failure and memory/idle sampling. Polling misses short-lived children, CPU tails and peaks; unavailable samples must not become zero-resource claims. Host memory includes harness/JIT/GC/report/setup allocation residue, and is not `Shovel.exe` memory. Deep offset starts graph lanes locally; exhaustive topology is tested only on the small fixture, with CLI sampling/counts on large fixtures. All trees are empty with two-parent diamond merges plus scope sentinels: no asset/worktree scans, wide/octopus DAG, million loose refs, cold disk/cache, user-repository, visible startup/input/paint, desktop idle, auth, Windows 10 or accessibility certification. Storage model/antivirus/physical RAM are not measured. No p95 or performance-budget pass claim is implemented.

Integration must review/register, build and execute small/full checks, then opt into the desired benchmarks, inspect reports/leftovers and update owning docs/TODO once with actual evidence. Release version/changelog/preview and any optimization are later integration work, not part of this commit.
