using System.IO;
using System.Net;
using System.Net.Http;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.Core.Git;
using GitGui.Core.Settings;
using GitGui.ProcessHost;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static bool _settingsPagesChecks;
    private static async Task RunSettingsPagesChecksAsync()
    {
        await Check("Categorized preferences save atomically and restore every exposed application choice", async () =>
        {
            var store = new SettingsStore(Fixture("pages-profile")); var shell = new ShellViewModel(persist: value => store.SaveAsync(value, default));
            shell.CompleteStartup(await store.LoadAsync(default));
            var model = new SettingsViewModel(shell.Preferences, true, shell.SaveSettingsAsync)
            {
                Appearance = AppAppearance.Light, ScalePercent = 120, DiffFontSize = 18, DiffSideBySide = true,
                AutoFetchEnabled = false, IntervalSeconds = "120", HistoryRefsRight = true,
                AvatarProvider = AvatarProvider.Initials, GiteaServer = "https://git.example.invalid/team",
                RiderExecutable = @"C:\Tools\rider.exe", CodeExecutable = @"C:\Tools\Code.exe", BashExecutable = @"C:\Tools\bash.exe",
                UpdateChannel = "win-beta", PeriodicUpdates = false
            };
            model.SaveCommand.Execute(null); await model.SaveTask;
            var loaded = await store.LoadAsync(default); var values = loaded.Settings;
            Expect(values.UiScale == 1.2 && values.Appearance == AppAppearance.Light && values.DiffFontSize == 18 && values.DiffSideBySide, "Display values persisted");
            Expect(values.HistoryRefsOnRight && !values.AutoFetchEnabled && values.AutoFetchIntervalSeconds == 120 && values.Avatars.Provider == AvatarProvider.Initials && values.Avatars.GiteaServer.EndsWith("/team"), "History/network preferences persisted");
            Expect(values.UpdateChannel == "win-beta" && !values.CheckUpdatesPeriodically && values.CodeExecutable == model.CodeExecutable && values.RiderExecutable == model.RiderExecutable && values.BashExecutable == model.BashExecutable, "Update and real tool overrides persisted");
            var restarted = new ShellViewModel(); restarted.CompleteStartup(loaded);
            Expect(restarted.Preferences.Avatars == values.Avatars && restarted.Preferences.UpdateChannel == "win-beta" && UiScale.Value == 1.2 && DiffPreferences.SideBySide, "Restart and runtime preferences agree");
            shell.BeginShutdown(); restarted.BeginShutdown(); UiScale.Set(1); DiffPreferences.Apply(new());
        });
        await Check("Invalid preferences and failed saves preserve edits without changing active display settings", async () =>
        {
            Expect((new AvatarSettings { GiteaServer = null! }).Normalize().GiteaServer == "", "A corrupt null endpoint falls back without losing the profile");
            var shell = new ShellViewModel(persist: _ => throw new IOException()); shell.CompleteStartup(new(new(), SettingsLoadStatus.Loaded));
            var model = new SettingsViewModel(shell.Preferences, true, shell.SaveSettingsAsync) { ScalePercent = 150, DiffFontSize = 20 };
            model.GiteaServer = "https://user:password@example.invalid"; Expect(!model.SaveCommand.CanExecute(null), "Credential-bearing endpoint rejected");
            model.GiteaServer = "https://git.example.invalid"; model.CodeExecutable = "relative.exe"; Expect(!model.SaveCommand.CanExecute(null), "Tool path validation is local");
            model.CodeExecutable = ""; model.SaveCommand.Execute(null); await model.SaveTask;
            Expect(model.Error.Length > 0 && model.ScalePercent == 150 && UiScale.Value == 1 && DiffPreferences.FontSize == 12, "Failed write retains edits and active values");
            shell.BeginShutdown();
        });
        for (var category = 0; category < 7; category++)
        {
            var page = category;
            await Check("Settings category " + page + " binds and fits normal/minimum Dark and Light layouts", async () =>
            {
                foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
                {
                    AppTheme.Apply(Application.Current.Resources, false, theme);
                    var writes = 0; var model = new SettingsViewModel(new(), true, _ => { writes++; return Task.FromResult(true); }) { SelectedCategory = page };
                    var window = new SettingsWindow(model); var content = (FrameworkElement)window.Content;
                    try
                    {
                        foreach (var size in new[] { (840, 650), (560, 380) })
                        {
                            Arrange(content, size.Item1, size.Item2); await Dispatcher.Yield(DispatcherPriority.ContextIdle);
                            Expect(((ListBox)window.FindName("CategoriesInput")).SelectedValue is int selected && selected == page, "Actual sidebar selects requested category");
                            var save = VisualDescendants<Button>(content).Single(button => ReferenceEquals(button.Command, model.SaveCommand));
                            var edge = save.TransformToAncestor(content).Transform(new Point(save.ActualWidth, save.ActualHeight));
                            Expect(save.Visibility == Visibility.Visible && save.ActualHeight > 0 && edge.Y <= size.Item2 && edge.X <= size.Item1, "Save is arranged inside the viewport outside scrolling content");
                            SaveRender(content, $"settings-page-{page}-{theme}-{size.Item1}.png", size.Item1, size.Item2);
                        }
                        Expect(writes == 0, "Opening and navigation do not save or prepare Git");
                    }
                    finally { window.Close(); }
                }
            });
        }
        AppTheme.Apply(Application.Current.Resources, false);
        await Check("Diff font preference reaches unified and side-by-side views and keeps defaults for new comparisons", () =>
        {
            DiffPreferences.Apply(new() { DiffFontSize = 19, DiffSideBySide = true });
            var row = AvatarFixtureRow(); var reader = new RepositoryReader(new WindowsProcessRunner());
            var model = new CommitFilesViewModel(reader, new("git", "fixture"), "fixture", row.Commit);
            var view = new CommitFilesView { DataContext = model }; Arrange(view, 950, 400);
            Expect(model.SideBySide && VisualDescendants<DataGrid>(view).Where(grid => grid.FontFamily.Source == "Consolas").All(grid => grid.FontSize == 19), "Actual text comparison grids consume saved font and mode");
            DiffPreferences.Apply(new()); return Task.CompletedTask;
        });
        await Check("Global identity uses real isolated Git config and rejects stale values and existing locks", async () =>
        {
            var runner = new WindowsProcessRunner(); var reader = new RepositoryReader(runner); var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity());
            var git = await reader.DiscoverAsync(null, default); var directory = Fixture("identity");
            var before = await actions.ReadGlobalIdentityAsync(git, directory, default);
            await actions.WriteGlobalIdentityAsync(git, directory, before, "Test 日本語", "test@example.invalid", default);
            var saved = await actions.ReadGlobalIdentityAsync(git, directory, default); Expect(saved.Name == "Test 日本語" && saved.Email == "test@example.invalid", "Native global identity round-trip");
            var identity = new GlobalIdentityViewModel(token => actions.ReadGlobalIdentityAsync(git, directory, token),
                (reviewed, name, email, token) => actions.WriteGlobalIdentityAsync(git, directory, reviewed, name, email, token));
            identity.LoadCommand.Execute(null); await identity.Pending; identity.Email = "";
            Expect(!identity.ApplyCommand.CanExecute(null) && identity.Status == GitGui.App.Localization.UiText.Get("SettingsIdentityInvalid"), "Disabled Apply explains invalid identity input");
            var stale = false; try { await actions.WriteGlobalIdentityAsync(git, directory, before, "Wrong", "wrong@example.invalid", default); } catch (GitActionException) { stale = true; }
            Expect(stale && await actions.ReadGlobalIdentityAsync(git, directory, default) == saved, "Stale review makes no changes");
            var config = Environment.GetEnvironmentVariable("GIT_CONFIG_GLOBAL")!; await File.WriteAllTextAsync(config + ".lock", "fixture");
            try
            {
                var failed = false; try { await actions.WriteGlobalIdentityAsync(git, directory, saved, "New", saved.Email, default); } catch (GitReadException) { failed = true; }
                Expect(failed && File.Exists(config + ".lock") && (await actions.ReadGlobalIdentityAsync(git, directory, default)).Name == saved.Name, "Git lock failure is retained and reported");
            }
            finally { File.Delete(config + ".lock"); }
        });
        await Check("Avatar provider resolves GitHub and subpath Gitea without credentials or unrelated-host requests", () =>
        {
            var hash = new string('a', 40);
            var github = AvatarService.Lookup(new(), "git@github.com:owner/repo.git", hash, "a@example.invalid");
            Expect(github?.Url.Host == "api.github.com" && github.Value.Url.AbsolutePath.EndsWith(hash), "SSH remote maps to exact commit API");
            var settings = new AvatarSettings { GiteaServer = "https://git.example.invalid/team" };
            var gitea = AvatarService.Lookup(settings, "https://git.example.invalid/team/owner/repo.git", hash, "a@example.invalid");
            Expect(gitea?.Url.AbsolutePath == "/team/api/v1/repos/owner/repo/git/commits/" + hash, "Configured subpath preserved");
            Expect(AvatarService.Lookup(settings, "https://elsewhere.invalid/owner/repo", hash, "a@example.invalid") is null, "Unknown remote does not receive requests");
            Expect(AvatarService.Lookup(new() { Provider = AvatarProvider.Initials }, "https://github.com/o/r", hash, "a@example.invalid") is null, "Initials disables networking");
            Expect(AvatarService.Lookup(new() { Provider = AvatarProvider.Gravatar }, "", hash, "  A@Example.invalid ") == AvatarService.Lookup(new() { Provider = AvatarProvider.Gravatar }, "", hash, "a@example.invalid"), "Gravatar email hash normalization");
            return Task.CompletedTask;
        });
        await Check("Avatar downloads decode images, deduplicate authors and preserve fallback on failure", async () =>
        {
            using var handler = new AvatarHandler(); using var service = new AvatarService(handler);
            var images = await Task.WhenAll(Enumerable.Range(0, 12).Select(_ => service.GetAsync(new(), "https://github.com/o/r", new string('a', 40), "same@example.invalid", default)));
            Expect(images.All(image => image is { IsFrozen: true }) && handler.Calls == 2 && !handler.Credentials, "One metadata/image request pair serves every repeated author");
            handler.Fail = true;
            Expect(await service.GetAsync(new(), "https://github.com/o/r", new string('b', 40), "missing@example.invalid", default) is null, "Network failure uses initials without throwing");
            handler.Fail = false; handler.Calls = 0;
            await Task.WhenAll(Enumerable.Range(0, 8).Select(index => service.GetAsync(new(), "https://github.com/o/r", new string('c', 40), index + "@example.invalid", default)));
            Expect(handler.Maximum <= 2, "At most two concurrent requests");
        });
        await Check("Gitea resolves a public author profile when the selected repository is private", async () =>
        {
            using var handler = new AvatarHandler { Gitea = true }; using var service = new AvatarService(handler);
            var settings = new AvatarSettings { GiteaServer = "https://git.example.invalid/team" };
            var image = await service.GetAsync(settings, "ssh://git@git.example.invalid/o/r.git", new string('a', 40), "test@example.invalid", default);
            Expect(image is not null && handler.Calls == 3 && !handler.Credentials, "Private commit falls back to exact public email match, then downloads the image");
            service.Reset(); handler.Fail = true;
            Expect(await service.GetAsync(settings, "ssh://git@git.example.invalid/o/r.git", new string('a', 40), "test@example.invalid", default) is null, "Preference reset invalidates image cache");
        });
        await Check("Settings channel change drains discovery without downloading or restarting", async () =>
        {
            var updater = new HeldSettingsUpdater(); var model = new UpdateViewModel(updater, () => throw new InvalidOperationException("No restart"));
            model.CheckCommand.Execute(null); await updater.Started.Task.WaitAsync(TimeSpan.FromSeconds(3));
            await model.ApplyPreferencesAsync(false, "win-beta");
            Expect(updater.Cancelled && model.Channel == "win-beta" && !model.PeriodicChecks && !model.Busy, "Old channel read is cancelled and the saved choice applies");
            model.Stop();
        });
        await Check("Cancelled avatar consumers cannot consume stale results or cancel another view's cached image", async () =>
        {
            using var handler = new AvatarHandler { Delay = 200 }; using var service = new AvatarService(handler); using var cancel = new CancellationTokenSource();
            var abandoned = service.GetAsync(new(), "https://github.com/o/r", new string('a', 40), "one@example.invalid", cancel.Token); cancel.Cancel();
            var cancelled = false; try { await abandoned; } catch (OperationCanceledException) { cancelled = true; }
            Expect(cancelled && await service.GetAsync(new(), "https://github.com/o/r", new string('a', 40), "one@example.invalid", default) is not null && handler.Calls == 2, "Shared download survives recycled consumer");
            var avatar = new AuthorAvatar { Author = AvatarFixtureRow(), Width = 46, Height = 46, Preferences = new() { Provider = AvatarProvider.Initials } };
            Arrange(avatar, 46, 46); Expect(VisualDescendants<TextBlock>(avatar).Single().Text == "G", "Initial is immediate and readable");
            SaveRender(avatar, "author-initial.png", 46, 46);
        });
    }
    private static CommitRow AvatarFixtureRow() => new(new(new(new string('a', 40), [], "Gleb", "gleb@example.invalid", DateTimeOffset.UnixEpoch, "Gleb", "gleb@example.invalid", DateTimeOffset.UnixEpoch, "Message"), new(0, 0, 1, false, [])));
    private sealed class AvatarHandler : HttpMessageHandler
    {
        public int Calls, Maximum, Delay = 15; public bool Fail, Credentials, Gitea; private int _active;
        private readonly byte[] _image;
        public AvatarHandler()
        {
            var bitmap = BitmapSource.Create(1, 1, 96, 96, PixelFormats.Bgra32, null, new byte[] { 40, 90, 150, 255 }, 4);
            var encoder = new PngBitmapEncoder(); encoder.Frames.Add(BitmapFrame.Create(bitmap)); using var stream = new MemoryStream(); encoder.Save(stream); _image = stream.ToArray();
        }
        protected override async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken token)
        {
            Interlocked.Increment(ref Calls); var active = Interlocked.Increment(ref _active); Maximum = Math.Max(Maximum, active);
            Credentials |= request.Headers.Authorization is not null || request.Headers.Contains("Cookie");
            try
            {
                await Task.Delay(Delay, token);
                if (Fail) return new(HttpStatusCode.NotFound);
                if (Gitea && request.RequestUri!.Host == "git.example.invalid")
                    return request.RequestUri.AbsolutePath.Contains("/users/search") ? new(HttpStatusCode.OK) { Content =
                        new StringContent("{\"data\":[{\"email\":\"test@example.invalid\",\"avatar_url\":\"https://images.example.invalid/avatar.png\"}]}", Encoding.UTF8, "application/json") } : new(HttpStatusCode.NotFound);
                return new(HttpStatusCode.OK) { Content = request.RequestUri!.Host == "api.github.com" ?
                    new StringContent("{\"author\":{\"avatar_url\":\"https://images.example.invalid/avatar.png\"}}", Encoding.UTF8, "application/json") : new ByteArrayContent(_image) };
            }
            finally { Interlocked.Decrement(ref _active); }
        }
    }
    private sealed class HeldSettingsUpdater : IAppUpdater
    {
        public UpdateConfiguration Configuration { get; private set; } = new("GlebHmarsky/Shovel", "fixture", "win-stable");
        public bool Available => true;
        public bool Cancelled;
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public void SelectChannel(string channel) => Configuration = Configuration with { Channel = channel };
        public async Task CheckAsync(IProgress<UpdateSnapshot> progress, CancellationToken token)
        {
            Started.TrySetResult();
            try { await Task.Delay(Timeout.Infinite, token); } catch (OperationCanceledException) { Cancelled = true; throw; }
        }
        public Task DownloadApprovedAsync(IProgress<UpdateSnapshot> progress, CancellationToken token) => throw new InvalidOperationException("No download");
        public Task VerifyReadyAsync(CancellationToken token) => throw new InvalidOperationException("No verification");
        public void Apply() => throw new InvalidOperationException("No install");
    }

}
