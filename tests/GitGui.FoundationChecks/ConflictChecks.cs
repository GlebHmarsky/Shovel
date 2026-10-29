using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Threading;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;
internal static partial class Program
{
    private static bool _conflictChecks, _usabilityChecks;
    private static async Task RunConflictChecksAsync()
    {
        var runner = new HistoryActionRunner(); var reader = new RepositoryReader(runner); var ids = new WindowsRepositoryIdentity();
        var actions = new GitActions(runner, reader, ids); var git = await reader.DiscoverAsync(null, default);
        async Task<string> Git(string repo, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args), default);
            Expect(result.ExitCode == 0, Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        async Task<string> Conflict(string name, bool rebase = false, bool delete = false, bool binary = false)
        {
            var repo = Fixture("conflict " + name); await Git(repo, "init", "-b", "main");
            await Git(repo, "config", "user.name", "Merge Fixture"); await Git(repo, "config", "user.email", "merge@example.invalid"); await Git(repo, "config", "core.autocrlf", "false");
            await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), binary ? [0, 1] : Encoding.UTF8.GetBytes("base\n"));
            await File.WriteAllTextAsync(Path.Combine(repo, "keep.txt"), "unrelated\n"); await Git(repo, "add", "."); await Git(repo, "commit", "-m", "base");
            await Git(repo, "switch", "-c", "incoming");
            if (delete) await Git(repo, "rm", "file.txt");
            else await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), binary ? [0, 2] : Encoding.UTF8.GetBytes("incoming\n"));
            await Git(repo, "commit", "-am", "incoming"); await Git(repo, "switch", "main");
            await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), binary ? [0, 3] : Encoding.UTF8.GetBytes("current\n")); await Git(repo, "commit", "-am", "current");
            var merge = await runner.RunAsync(new(git.Executable, repo, rebase ? ["rebase", "incoming"] : ["merge", "--no-edit", "incoming"]), default);
            Expect(merge.ExitCode != 0, "Fixture must conflict"); return repo;
        }
        async Task<ConflictPreview> Preview(string repo)
        {
            var context = await actions.ReadWorkingCopyAsync(git, repo, default);
            return await actions.ReadConflictAsync(context, context.Changes.Entries.Single(e => e.Group == ChangeGroup.Conflicted), default);
        }
        Task<WorkingCopyResult> Resolve(ConflictPreview p, ConflictChoice choice, string text = "") => actions.ExecuteWorkingCopyAsync(p.Context,
            new(WorkingCopyAction.ResolveConflict, [p.Change], Resolution: new(p, choice, text)), default);
        async Task Reject(string code, Func<Task> action)
        { try { await action(); throw new InvalidOperationException("Expected " + code); } catch (GitActionException error) { Expect(error.Code == code, error.Code); } }

        await Check("Merge editor reads named stages; checkbox selection is inert; edited resolution stages exact result", async () =>
        {
            var repo = await Conflict("editor"); var p = await Preview(repo);
            Expect(p.CanEdit && p.Current?.Text == "current\n" && p.Incoming?.Text == "incoming\n" && p.Base?.Text == "base\n" && p.IncomingSource.StartsWith("incoming · ", StringComparison.Ordinal), "Actual base/current/incoming and source branch/commit");
            var model = new ConflictViewModel(p.Context, p.Change, actions); var window = new ConflictWindow(model);
            try
            {
                await model.StartAsync(); var calls = runner.Calls; model.Current = true; model.Incoming = true;
                Expect(runner.Calls == calls && model.ResolveCommand.CanExecute(null), "Choosing sides performs no write/read");
                foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
                {
                    AppTheme.Apply(Application.Current.Resources, false, theme);
                    foreach (var size in new[] { (1100, 740), (800, 570) })
                    { Arrange((FrameworkElement)window.Content, size.Item1, size.Item2); SaveRender((FrameworkElement)window.Content, $"conflict-{theme}-{size.Item1}.png", size.Item1, size.Item2); }
                }
                model.ResolveCommand.Execute(null); await model.Work;
                Expect(!model.Resolved && model.ResolveCommand.CanExecute(null), "Unedited markers reject without losing retry/editor state");
                model.Text = "current and incoming\n"; model.ResolveCommand.Execute(null); await model.Work;
                Expect(model.Resolved && await Git(repo, "show", ":file.txt") == "current and incoming" && await Git(repo, "ls-files", "-u") == "", "Edited content really staged");
                Expect(await File.ReadAllTextAsync(Path.Combine(repo, "keep.txt")) == "unrelated\n", "Unrelated file preserved");
            }
            finally { await model.CloseAsync(); window.Close(); AppTheme.Apply(Application.Current.Resources, false); }
        });
        await Check("Rebase names rebased and replayed sides correctly and resolves actual stage 2", async () =>
        {
            var repo = await Conflict("rebase", rebase: true); var p = await Preview(repo);
            Expect(p.Context.Repository.Recovery?.Kind == RecoveryKind.Rebase && p.Current?.Text == "incoming\n" && p.Incoming?.Text == "current\n", "Rebase stage reversal");
            var result = await Resolve(p, ConflictChoice.Current);
            Expect(result.Result.Success && await Git(repo, "show", ":file.txt") == "incoming" && await Git(repo, "ls-files", "-u") == "", "Stage 2 applied and resolved");
        });
        await Check("Modify/delete chooses absent incoming side and stages deletion", async () =>
        {
            var repo = await Conflict("deletion", delete: true); var p = await Preview(repo);
            Expect(p.Incoming is null, "Absent incoming side"); var result = await Resolve(p, ConflictChoice.Incoming);
            Expect(result.Result.Success && !File.Exists(Path.Combine(repo, "file.txt")) && await Git(repo, "ls-files", "-u") == "", "Deletion resolved");
        });
        await Check("Binary side choice preserves bytes without allowing text edits", async () =>
        {
            var repo = await Conflict("binary", binary: true); var p = await Preview(repo);
            Expect(!p.CanEdit, "Binary is not text"); var result = await Resolve(p, ConflictChoice.Incoming);
            Expect(result.Result.Success && (await File.ReadAllBytesAsync(Path.Combine(repo, "file.txt"))).SequenceEqual(new byte[] { 0, 2 }), "Incoming binary bytes");
        });
        await Check("Conflict rejects external edits and residual markers; cancelled writes retain index", async () =>
        {
            var repo = await Conflict("stale"); var p = await Preview(repo); var index = p.Context.IndexStamp;
            await Reject("ConflictMarkersRemain", () => Resolve(p, ConflictChoice.Edited, p.WorkingText!));
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "external edit\n");
            await Reject("ConflictEditedOutside", () => Resolve(p, ConflictChoice.Current));
            using var cancel = new CancellationTokenSource(); cancel.Cancel();
            await ExpectException<OperationCanceledException>(() => actions.ExecuteWorkingCopyAsync(p.Context,
                new(WorkingCopyAction.ResolveConflict, [p.Change], Resolution: new(p, ConflictChoice.Incoming)), cancel.Token));
            Expect(await GitActions.IndexStampAsync(p.Context.Repository, default) == index, "Unmerged index preserved");
        });
        await Check("Stash conflicts label unknown provenance and resolve with no fake sequence", async () =>
        {
            var repo = await Conflict("stash"); await Git(repo, "merge", "--abort");
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "saved\n"); await Git(repo, "stash", "push");
            await File.WriteAllTextAsync(Path.Combine(repo, "file.txt"), "later\n"); await Git(repo, "commit", "-am", "later");
            await runner.RunAsync(new(git.Executable, repo, ["stash", "apply"]), default);
            var p = await Preview(repo); Expect(p.IncomingSource == "" && p.Context.Repository.Recovery is null, "No inferred stash branch/sequence");
            Expect((await Resolve(p, ConflictChoice.Incoming)).Result.Success && await Git(repo, "show", ":file.txt") == "saved", "Stash side resolved");
        });
        await Check("Merge tool failure and index lock retain unresolved state and edited file", async () =>
        {
            var repo = await Conflict("tool"); await Git(repo, "config", "merge.tool", "fixture");
            await Git(repo, "config", "mergetool.fixture.cmd", "exit 1"); await Git(repo, "config", "mergetool.fixture.trustExitCode", "true");
            var p = await Preview(repo); var result = await Resolve(p, ConflictChoice.External);
            Expect(!result.Result.Success && (await Git(repo, "ls-files", "-u")).Length > 0, "Failed tool remains unresolved");
            p = await Preview(repo); var lockPath = Path.Combine(repo, ".git", "index.lock"); await File.WriteAllTextAsync(lockPath, "fixture");
            result = await Resolve(p, ConflictChoice.Edited, "retained edit\n");
            Expect(!result.Result.Success && await File.ReadAllTextAsync(Path.Combine(repo, "file.txt")) == "retained edit\n" && File.Exists(lockPath), "Failed add retains edited result and foreign lock");
        });
    }
}
