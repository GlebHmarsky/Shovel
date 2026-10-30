using System.ComponentModel;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Windows;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.App.ViewModels;
using GitGui.App.Views;
using GitGui.App.Services;
using GitGui.Core.Git;
using GitGui.ProcessHost;
using GitGui.Core.Diagnostics;
using GitGui.Core.Settings;

namespace GitGui.App;

/// <summary>Composition root and lifetime owner. Views contain only presentation behavior.</summary>
public partial class App : Application
{
    private readonly CancellationTokenSource _startupCancellation = new();
    private Mutex? _profileMutex;
    private bool _ownsProfile;
    private AppTheme? _theme;
    private DiagnosticLog? _log;
    private SettingsStore? _settings;
    private ShellViewModel? _shell;
    private CancellationTokenSource _updateCancellation = new();
    private AppUpdater? _updater;
    private UpdateViewModel? _updates;
    private Task _automaticUpdates = Task.CompletedTask;
    private Task _startupTask = Task.CompletedTask;
    private bool _closing;
    private bool _allowClose;
    private bool _fatal;
    private readonly PerformanceTrace _trace = new();
    private readonly SemaphoreSlim _settingsWrites = new(1, 1);

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);
        DispatcherUnhandledException += OnDispatcherException;
        AppDomain.CurrentDomain.UnhandledException += OnDomainException;
        try
        {
            _theme = new AppTheme(this);
            var directory = ResolveDataDirectory(e.Args);
            // One writer per local user/profile; the hash keeps the path out of the mutex name.
            var identity = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(directory.ToUpperInvariant())));
            _profileMutex = new Mutex(false, @"Local\GitGui.Foundation." + identity);
            try { _ownsProfile = _profileMutex.WaitOne(0); }
            catch (AbandonedMutexException) { _ownsProfile = true; }
            if (!_ownsProfile)
            {
                MessageBox.Show(UiText.Get("AlreadyRunning"), UiText.Get("AppTitle"),
                    MessageBoxButton.OK, MessageBoxImage.Information);
                Shutdown();
                return;
            }

            _log = new DiagnosticLog(directory);
            _log.Record(DiagnosticEvent.Started);
            _settings = new SettingsStore(directory);
            var runner = new WindowsProcessRunner();
            var reader = new RepositoryReader(runner, _trace);
            var identities = new WindowsRepositoryIdentity();
            _shell = new ShellViewModel(reader, new UserInteraction(), identities,
                persist: SaveIdentitySettingsAsync, trace: _trace, actions: new GitActions(runner, reader, identities));
            _updater = new AppUpdater(UpdateConfiguration.Load());
            _updates = new UpdateViewModel(_updater, RestartForUpdateAsync, () =>
                _shell.LoadedSettings is { CanSave: false }
                    ? UiText.Get("UpdateSettingsBlocked") + " " + _shell.SettingsNotice : null);
            _shell.ConfigureUpdates(_updates);
            _shell.DraftStore = new GitGui.Core.Drafts.CommitDraftStore(null, identities, _log.RecordDraftStorage);
            _shell.AppearanceChanged += (_, _) => _theme.Select(_shell.Appearance);
            MainWindow window;
            using (_trace.Measure(TimingStage.Shell)) window = new MainWindow(_shell);
            MainWindow = window;
            window.Closing += OnWindowClosing;
            window.Show();
            _startupTask = InitializeAsync(window, _startupCancellation.Token);
        }
        catch (Exception exception) { FailAndExit(exception); }
    }

    private async Task InitializeAsync(MainWindow window, CancellationToken cancellationToken)
    {
        try
        {
            // File opening, JSON parsing and directory work must also stay off the dispatcher.
            SettingsLoadResult result;
            using (_trace.Measure(TimingStage.Settings)) result = await Task.Run(() => _settings!.LoadAsync(cancellationToken), cancellationToken);
            if (_closing || _fatal) return;
            window.RestoreLayout(result.Settings);
            _shell!.CompleteStartup(result);
            _automaticUpdates = _updates!.RunAutomaticChecksAsync(_updateCancellation.Token);
            _log!.Record(result.CanSave ? DiagnosticEvent.SettingsLoaded : DiagnosticEvent.SettingsReadOnly);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested) { }
        catch (Exception exception) { FailAndExit(exception); }
    }

    private async Task SaveIdentitySettingsAsync(AppSettings session)
    {
        if (MainWindow is not MainWindow window || _shell?.LoadedSettings?.CanSave != true)
            throw new IOException("SettingsNotWritable");
        var snapshot = _shell.CaptureSession(window.CaptureLayout());
        await _settingsWrites.WaitAsync();
        try
        {
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
            await Task.Run(() => _settings!.SaveAsync(snapshot, timeout.Token), timeout.Token);
        }
        finally { _settingsWrites.Release(); }
    }

    private async void OnWindowClosing(object? sender, CancelEventArgs e)
    {
        if (_allowClose) return;
        e.Cancel = true;
        if (_closing || sender is not MainWindow window) return;
        if (_shell!.ActionOpen) { _shell.ShowActionDetailsCommand.Execute(null); return; } // Finish/cancel and reconcile before shutdown.
        _closing = true;
        _shell!.BeginShutdown();
        _startupCancellation.Cancel();
        try
        {
            _updateCancellation.Cancel();
            _updates?.Stop();
            if (_updates is not null) await _updates.ActiveTask;
            await _automaticUpdates;
            await _startupTask;
            await _shell.WaitForIdleAsync();
            await _shell.WorkspaceToolTask;
            await _shell.PreferenceTask;
            if (_fatal) return;
            if (_shell.LoadedSettings?.CanSave == true)
            {
                var snapshot = _shell.CaptureSession(window.CaptureLayout());
                try
                {
                    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
                    await Task.Run(() => _settings!.SaveAsync(snapshot, timeout.Token), timeout.Token);
                    _log!.Record(DiagnosticEvent.SettingsSaved);
                }
                catch (Exception exception) when (exception is IOException or UnauthorizedAccessException or OperationCanceledException)
                {
                    _log!.Record(DiagnosticEvent.SettingsSaveFailed, exception);
                    var answer = MessageBox.Show(window, UiText.Get("SettingsSaveFailedQuestion"), UiText.Get("AppTitle"),
                        MessageBoxButton.YesNo, MessageBoxImage.Warning, MessageBoxResult.No);
                    if (answer != MessageBoxResult.Yes)
                    {
                        _closing = false;
                        _shell.ResumeAfterSaveFailure();
                        _updates?.Resume();
                        _updateCancellation.Dispose();
                        _updateCancellation = new();
                        _automaticUpdates = _updates!.RunAutomaticChecksAsync(_updateCancellation.Token);
                        return;
                    }
                }
            }
            _log!.Record(DiagnosticEvent.Stopped);
            // Diagnostics are best effort and must never indefinitely hold shutdown open.
            try { await _log.DisposeAsync().AsTask().WaitAsync(TimeSpan.FromSeconds(2)); }
            catch (TimeoutException) { }
            _allowClose = true;
            window.Close();
            Shutdown();
        }
        catch (Exception exception) { FailAndExit(exception); }
    }

    private async Task<bool> RestartForUpdateAsync()
    {
        if (_closing || _fatal || _shell?.CanPrepareUpdate != true || MainWindow is not MainWindow window) return false;
        _closing = true;
        _shell.BeginShutdown(); // Close command admission before any await or final repository inspection.
        var marked = false;
        try
        {
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(90));
            await _shell.WaitForIdleAsync().WaitAsync(timeout.Token);
            await _shell.PreferenceTask.WaitAsync(timeout.Token);
            if (_shell.ToolsNeedUpdateAcknowledgment && MessageBox.Show(window, UiText.Get("UpdateToolsQuestion"),
                UiText.Get("UpdateTitle"), MessageBoxButton.YesNo, MessageBoxImage.Question, MessageBoxResult.No) != MessageBoxResult.Yes) return false;
            if (!await _shell.VerifyUpdateIdleAsync(timeout.Token)) return false;
            var snapshot = _shell.CaptureSession(window.CaptureLayout());
            await _settingsWrites.WaitAsync(timeout.Token);
            try { await Task.Run(() => _settings!.SaveAsync(snapshot, timeout.Token), timeout.Token); }
            finally { _settingsWrites.Release(); }
            await Task.Run(() => _updater!.VerifyReadyAsync(timeout.Token), timeout.Token);
            if (!await _shell.VerifyUpdateIdleAsync(timeout.Token)) return false;
            // Prevent another installed launch in the exit/apply gap. The framework's restarted hook clears it.
            await Task.Run(Program.MarkHandoff, timeout.Token); marked = true;
            _updater!.Apply();
            _startupCancellation.Cancel();
            _updateCancellation.Cancel();
            _updates!.Stop();
            _allowClose = true;
            Shutdown();
            return true;
        }
        finally
        {
            if (!_allowClose)
            {
                if (marked) Program.CancelHandoff();
                _closing = false;
                _shell.ResumeAfterSaveFailure();
            }
        }
    }

    private static string ResolveDataDirectory(string[] arguments)
    {
        if (arguments.Length == 0)
            return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "GitGui", "Foundation");
        if (arguments.Length == 2 && arguments[0] == "--data-directory" && Path.IsPathFullyQualified(arguments[1]))
            return Path.TrimEndingDirectorySeparator(Path.GetFullPath(arguments[1]));
        throw new ArgumentException("InvalidStartupArguments");
    }

    private void OnDispatcherException(object sender, DispatcherUnhandledExceptionEventArgs e)
    {
        e.Handled = true;
        FailAndExit(e.Exception);
    }

    private void OnDomainException(object sender, UnhandledExceptionEventArgs e)
    {
        // The runtime will terminate. Do not attempt UI recovery or serialize possibly invalid state.
        _log?.Record(DiagnosticEvent.FatalError, e.ExceptionObject as Exception);
    }

    private void FailAndExit(Exception exception)
    {
        if (_fatal) return;
        _fatal = true;
        _log?.Record(DiagnosticEvent.FatalError, exception);
        MessageBox.Show(UiText.Get("FatalErrorMessage"), UiText.Get("AppTitle"),
            MessageBoxButton.OK, MessageBoxImage.Error);
        _allowClose = true;
        Shutdown(1);
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _theme?.Dispose();
        _updates?.Stop();
        _updater?.Dispose();
        _updateCancellation.Cancel();
        _updateCancellation.Dispose();
        _startupCancellation.Cancel();
        _startupCancellation.Dispose();
        if (_ownsProfile) _profileMutex!.ReleaseMutex();
        _profileMutex?.Dispose();
        base.OnExit(e);
    }
}
