using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Processes;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunFileMenuChecksAsync()
    {
        var runner = new FileMenuRunner(new WindowsProcessRunner()); var reader = new RepositoryReader(runner);
        var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity()); var git = await reader.DiscoverAsync(null, default);
        var repo = Fixture("file menu 日本語 comma,"); var interaction = new FileMenuInteraction();
        async Task<string> Git(params string[] arguments)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, arguments), default);
            Expect(result.ExitCode == 0, "Fixture Git: " + Encoding.UTF8.GetString(result.StandardError)); return Encoding.UTF8.GetString(result.StandardOutput).Trim();
        }
        await Git("init", "-b", "main"); await Git("config", "user.name", "Menu Fixture"); await Git("config", "user.email", "menu@example.invalid");
        await Git("config", "core.autocrlf", "false");
        var names = new[] { "a 日本語, [literal].txt", "b.txt", "deleted.txt" };
        foreach (var name in names) await File.WriteAllTextAsync(Path.Combine(repo, name), "original\n");
        await Git("add", "."); await Git("commit", "-m", "base");
        foreach (var name in names.Take(2)) await File.WriteAllTextAsync(Path.Combine(repo, name), "changed\n");
        File.Delete(Path.Combine(repo, names[2])); await File.WriteAllTextAsync(Path.Combine(repo, "-new.txt"), "new\n");
        var model = new CommitViewModel(repo, git, reader, actions, interaction); await model.StartLoading(); await model.DiffTask;
        var window = new CommitWindow(model); var content = (FrameworkElement)window.Content;
        try
        {
            Arrange(content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle); await model.DiffTask;
            var unstaged = (DataGrid)window.FindName("UnstagedTable"); var staged = (DataGrid)window.FindName("StagedTable");
            DataGridRow Row(DataGrid table, ChangeRow item) { table.ScrollIntoView(item); table.UpdateLayout(); return (DataGridRow)table.ItemContainerGenerator.ContainerFromItem(item); }
            static void RightClick(DataGridRow row) => row.RaiseEvent(new MouseButtonEventArgs(Mouse.PrimaryDevice, 0, MouseButton.Right) { RoutedEvent = UIElement.PreviewMouseRightButtonDownEvent });
            static ChangeFileMenu MenuModel(ContextMenu menu) => (ChangeFileMenu)menu.DataContext;
            static void CloseMenu(ContextMenu menu) => menu.RaiseEvent(new RoutedEventArgs(ContextMenu.ClosedEvent));
            async Task Idle() { await model.OperationTask; await model.LoadingTask; await model.DiffTask; }

            await Check("File context menu preserves multiselection, targets another row and opens without Git or file handoff", async () =>
            {
                var first = model.Unstaged.Single(r => r.Change.Path == names[0]); var second = model.Unstaged.Single(r => r.Change.Path == names[1]);
                unstaged.SelectedItems.Clear(); unstaged.SelectedItems.Add(first); unstaged.SelectedItems.Add(second); await model.DiffTask;
                var row = Row(unstaged, second); var calls = runner.Calls; var watch = Stopwatch.StartNew(); RightClick(row);
                var menu = window.PrepareFileMenu(row); Arrange(menu, 340, 220); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                Expect(unstaged.SelectedItems.Count == 2 && runner.Calls == calls && interaction.OpenCalls == 0, "Preserve containing selection, no menu preparation calls");
                Console.WriteLine($"File menu construction/layout: {watch.Elapsed.TotalMilliseconds:F0} ms; zero Git calls");
                Expect(!MenuModel(menu).OpenCommand.CanExecute(null) && MenuModel(menu).StageCommand.CanExecute(null), "Single-file opening and multi-file staging states");
                MenuModel(menu).CopyCommand.Execute(null); Expect(interaction.Clipboard.Split(Environment.NewLine).ToHashSet().SetEquals(names.Take(2)), "Copy exactly selected relative paths");
                SaveRender(menu, "local-changes-file-menu.png", 340, 220); CloseMenu(menu); await model.DiffTask;
                row = Row(unstaged, model.Unstaged.Single(r => r.Change.Path == "-new.txt")); calls = runner.Calls; RightClick(row); menu = window.PrepareFileMenu(row);
                Arrange(menu, 340, 220); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                Expect(unstaged.SelectedItems.Count == 1 && runner.Calls == calls, "Different row replaces selection without starting its diff until menu closes");
                MenuModel(menu).CopyFullCommand.Execute(null); Expect(interaction.Clipboard == Path.Combine(repo, "-new.txt"), "Full path targets clicked row");
                SaveRender(menu, "local-changes-single-menu.png", 340, 220);
                CloseMenu(menu); await model.DiffTask; Expect(model.DiffTitle == "-new.txt", "Deferred selection diff follows menu close");
                Expect(unstaged.ContextMenu is null, "Blank table background has no inherited file menu");
            });
            await Check("Bound context commands stage selected files despite Shift, unstage all, and retain draft/worktree", async () =>
            {
                var first = model.Unstaged.Single(r => r.Change.Path == names[0]); var row = Row(unstaged, first); RightClick(row);
                var menu = window.PrepareFileMenu(row); Arrange(menu, 340, 220); model.ShiftHeld = true; model.Subject = "keep draft";
                var stage = menu.Items.OfType<MenuItem>().Single(item => Equals(item.Header, UiText.Get("CommitStage")));
                Expect(ReferenceEquals(stage.Command, MenuModel(menu).StageCommand), "Real menu command binding");
                stage.Command.Execute(null); CloseMenu(menu); await Idle();
                Expect(await Git("diff", "--cached", "--name-only") == names[0] || model.Staged.Single().Change.Path == names[0], "Only selected file staged; Shift does not expand targets");
                Expect(model.Staged.Count == 1 && model.Subject == "keep draft" && await File.ReadAllTextAsync(Path.Combine(repo, names[0])) == "changed\n", "Real index result, retained file and draft");
                row = Row(staged, model.Staged.Single()); menu = window.PrepareFileMenu(row); Arrange(menu, 340, 220);
                Expect(MenuModel(menu).StageLabel == UiText.Get("CommitUnstage") && MenuModel(menu).AllLabel == UiText.Get("CommitUnstageAll"), "Staged menu labels reflect direction");
                MenuModel(menu).AllCommand.Execute(null); CloseMenu(menu); await Idle();
                Expect(await Git("diff", "--cached", "--name-only") == "" && model.Staged.Count == 0, "Real unstage all");
                model.ShiftHeld = false;
                row = Row(unstaged, model.Unstaged[0]); menu = window.PrepareFileMenu(row); MenuModel(menu).AllCommand.Execute(null); CloseMenu(menu); await Idle();
                Expect(model.Staged.Count == 4 && model.Unstaged.Count == 0, "Stage all includes observed additions and deletion");
                row = Row(staged, model.Staged[0]); menu = window.PrepareFileMenu(row); Arrange(menu, 340, 220); SaveRender(menu, "local-changes-staged-menu.png", 340, 220); CloseMenu(menu);
            });
            await Check("Refreshed menus reject stale writes; copy errors and file handoff failures stay localized", async () =>
            {
                var row = Row(staged, model.Staged.First(r => r.Change.Code != 'D')); var expectedPath = ((ChangeRow)row.Item).Change.Path;
                var menu = window.PrepareFileMenu(row); var old = MenuModel(menu);
                await model.StartLoading(); var calls = runner.Calls;
                Expect(!old.StageCommand.CanExecute(null) && old.MutationHelp == UiText.Get("FileMenuStale"), "Refresh invalidates captured mutation targets");
                old.StageCommand.Execute(null); Expect(runner.Calls == calls, "Stale menu cannot retarget current list");
                interaction.FailCopy = true; old.CopyCommand.Execute(null); Expect(model.Status == UiText.Get("FileMenuCopyFailed"), "Clipboard error visible without exception text"); interaction.FailCopy = false;
                interaction.FailOpen = true; old.OpenCommand.Execute(null); await model.FileTask;
                Expect(model.Status == UiText.Get("FileMenuOpenFailed") && interaction.Request?.Relative == expectedPath, "Working-file request/error belongs to captured row"); interaction.FailOpen = false;
                old.ExplorerCommand.Execute(null); await model.FileTask; Expect(interaction.Request?.Explorer == true && runner.Calls == calls, "Explorer dispatch starts no Git");
                CloseMenu(menu); await model.DiffTask;
                Arrange(content, 1200, 760); SaveRender(content, "local-changes-menu-workspace.png", 1200, 760);
                Arrange(content, 900, 580); SaveRender(content, "local-changes-menu-minimum.png", 900, 580);
            });
            await Check("File targets preserve literal paths, reject escape/metadata paths and handle deleted files without shell execution", async () =>
            {
                var target = await Task.Run(() => FileShellActions.PrepareTarget(repo, names[0], true));
                Expect(target.Path == Path.Combine(repo, names[0]) && target.SelectInExplorer, "Unicode/comma/bracket filename stays a structured path");
                target = await Task.Run(() => FileShellActions.PrepareTarget(repo, "gone/sub/file.txt", true));
                Expect(target.Path == repo && target.MissingFileFolder, "Missing nested path opens nearest containing folder");
                foreach (var bad in new[] { "../outside.txt", ".git/config", Path.Combine(repo, names[0]) })
                    await ExpectException<GitActionException>(() => Task.Run(() => FileShellActions.PrepareTarget(repo, bad, true)));
                await ExpectException<GitActionException>(() => Task.Run(() => FileShellActions.PrepareTarget(repo, "deleted.txt", false)));
                var deleted = model.Staged.Single(r => r.Change.Code == 'D'); var menu = window.PrepareFileMenu(Row(staged, deleted));
                Expect(!MenuModel(menu).OpenCommand.CanExecute(null) && MenuModel(menu).ExplorerCommand.CanExecute(null), "Deleted file has explained Open unavailability and working folder route"); CloseMenu(menu);
            });
            await Check("Closing Local Changes cancels a pending file handoff without publishing a late result", async () =>
            {
                var row = Row(staged, model.Staged.First(r => r.Change.Code != 'D')); var menu = window.PrepareFileMenu(row); var commands = MenuModel(menu);
                interaction.HoldOpen = true; commands.OpenCommand.Execute(null);
                Expect(!commands.OpenCommand.CanExecute(null) && model.Status == UiText.Get("FileMenuOpening"), "One visible pending handoff per modal");
                CloseMenu(menu); await model.CloseAsync().WaitAsync(TimeSpan.FromSeconds(3));
                Expect(!commands.OpenCommand.CanExecute(null) && model.FileTask.IsCompleted, "Close drains cancellable handoff and disables closed-model commands");
            });
        }
        finally { await model.CloseAsync(); window.Close(); }
    }
    private sealed class FileMenuRunner(IProcessRunner inner) : IProcessRunner
    {
        public int Calls;
        public Task<ProcessResult> RunAsync(ProcessRequest request, CancellationToken token, Action<ProcessOutput>? output = null)
        { Interlocked.Increment(ref Calls); return inner.RunAsync(request, token, output); }
    }
    private sealed class FileMenuInteraction : IUserInteraction
    {
        public string Clipboard = ""; public bool FailCopy, FailOpen, HoldOpen; public int OpenCalls;
        public (string Root, string Relative, bool Explorer)? Request;
        public string? ChooseRepository() => null; public string? ChooseGit() => null;
        public void CopyFilePaths(string text) { if (FailCopy) throw new System.Runtime.InteropServices.COMException(); Clipboard = text; }
        public async Task<bool> OpenWorkingFileAsync(string root, string relative, bool explorer, CancellationToken token)
        { OpenCalls++; Request = (root, relative, explorer); if (FailOpen) throw new IOException(); if (HoldOpen) await Task.Delay(Timeout.Infinite, token); return false; }
    }
}
