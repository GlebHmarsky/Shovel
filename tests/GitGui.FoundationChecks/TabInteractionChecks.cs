using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Settings;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static async Task RunTabInteractionChecksAsync()
    {
        await Check("Tab commands target the clicked repository and middle click closes an inactive tab", async () =>
        {
            var paths = Enumerable.Range(0, 3).Select(i => Fixture("tab-actions-" + i)).ToArray();
            var runner = new ModelRunner(paths[0]) { UseRequestDirectory = true }; runner.Release.TrySetResult();
            var model = new ShellViewModel(new(runner), new ModelInteraction(paths[0]), new ModelIdentities());
            model.CompleteStartup(new(new() { RepositoryPaths = paths }, SettingsLoadStatus.Loaded)); await model.WaitForIdleAsync();
            var selected = model.SelectedRepository!; var inactive = model.Repositories[1];
            var window = new MainWindow(model);
            try
            {
                Arrange((FrameworkElement)window.Content, 1200, 760); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                var list = VisualDescendants<ListBox>((FrameworkElement)window.Content).Single(item => item.ItemsSource == model.Repositories);
                var container = (ListBoxItem)list.ItemContainerGenerator.ContainerFromItem(inactive);
                var menu = container.ContextMenu; menu.PlacementTarget = container;
                Arrange(menu, 230, 220); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                var close = menu.Items.OfType<MenuItem>().Single(item => Equals(item.Header, UiText.Get("CloseTab")));
                Expect(ReferenceEquals(close.Command, model.CloseTabCommand) && ReferenceEquals(close.CommandParameter, inactive), "Context menu binds its placement tab rather than selected tab");
                SaveRender(menu, "repository-tab-menu.png", 230, 220);
                var calls = runner.Calls;
                model.MoveTabRightCommand.Execute(inactive);
                Expect(model.Repositories[2] == inactive && model.SelectedRepository == selected && runner.Calls == calls, "Inactive movement does not load or select the tab");
                // Routed WPF event only: no system input, desktop control or visible window.
                var middle = new MouseButtonEventArgs(Mouse.PrimaryDevice, 0, MouseButton.Middle) { RoutedEvent = UIElement.PreviewMouseDownEvent };
                container.RaiseEvent(middle);
                Expect(middle.Handled && !model.Repositories.Contains(inactive) && model.SelectedRepository == selected && runner.Calls == calls, "Middle click closes only the clicked inactive tab");
                var neighbor = model.Repositories[1];
                model.CloseOtherTabsCommand.Execute(neighbor); await model.WaitForIdleAsync();
                Expect(model.Repositories.Count == 1 && model.SelectedRepository == neighbor && model.Commits.Count == 1, "Close others keeps and loads its target");
                model.CloseTabCommand.Execute(neighbor); await model.WaitForIdleAsync();
                Expect(model.Repositories.Count == 0 && model.SelectedRepository is null && model.Commits.Count == 0 && !model.IsBusy, "Closing last tab leaves a ready empty workspace");
            }
            finally { window.Close(); }
        });
        await Check("Closing an active tab cancels its read and cannot strand busy state or publish late rows", async () =>
        {
            var path = Fixture("close-busy-tab");
            var runner = new ModelRunner(path) { HoldHistory = true, IgnoreHistoryCancellation = true }; runner.Release.TrySetResult();
            var model = CreateModel(runner, path); model.OpenRepositoryCommand.Execute(null);
            await runner.HistoryStarted.Task.WaitAsync(TimeSpan.FromSeconds(5));
            var inactive = new RepositoryTab(Fixture("close-inactive-during-read")); model.Repositories.Add(inactive);
            model.CloseTabCommand.Execute(inactive);
            Expect(model.IsBusy && !runner.Cancelled && model.SelectedRepository?.Directory == path, "Closing an inactive tab does not cancel active work");
            model.CloseTabCommand.Execute(model.SelectedRepository);
            runner.ReleaseHistory.TrySetResult(); await model.WaitForIdleAsync();
            Expect(!model.IsBusy && model.SelectedRepository is null && model.Commits.Count == 0 && model.OpenRepositoryCommand.CanExecute(null), "Cancelled closed-tab result cannot reach an empty workspace");
        });
    }
}
