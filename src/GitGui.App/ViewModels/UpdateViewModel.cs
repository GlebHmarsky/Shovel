using GitGui.App.Localization;
using GitGui.App.Services;

namespace GitGui.App.ViewModels;

public sealed class UpdateViewModel : ObservableObject
{
    private readonly IAppUpdater _updater;
    private readonly Func<Task<bool>> _restart;
    private readonly Func<string?>? _blockedReason;
    private CancellationTokenSource? _cancel;
    private UpdateSnapshot _snapshot;
    private bool _periodic = true, _stopping, _busy, _launchCheckStarted, _windowOpened;
    public Task ActiveTask { get; private set; } = Task.CompletedTask;
    public RelayCommand PrimaryCommand { get; }
    public string PrimaryLabel => UiText.Get(Ready || Offered ? "UpdateApprove" : "UpdateRetry");
    public RelayCommand CheckCommand { get; }
    public RelayCommand CancelCommand { get; }
    public RelayCommand RestartCommand { get; }
    public UpdateConfiguration Configuration => _updater.Configuration;
    public string InstalledVersion => typeof(UpdateViewModel).Assembly.GetName().Version?.ToString(3) ?? "";
    public string Endpoint => Configuration.IsConfigured ? $"https://github.com/{Configuration.Repository}/releases" : UiText.Get("UpdateUnconfigured");
    public string Channel => Configuration.Channel;
    public bool CanSelectChannel => !_busy && !_stopping;
    public int SelectedChannel
    {
        get => Channel == "win-beta" ? 1 : 0;
        set
        {
            var channel = value == 1 ? "win-beta" : "win-stable";
            if (!CanSelectChannel || channel == Channel) return;
            _updater.SelectChannel(channel);
            Publish(new(_updater.Available ? UpdatePhase.Idle : UpdatePhase.Portable));
            Changed(nameof(Channel)); Changed(nameof(SelectedChannel));
            if (_windowOpened) ActiveTask = CheckAsync();
        }
    }
    public string Notes => _snapshot.Notes;
    public bool Ready => _snapshot.Phase is UpdatePhase.Ready or UpdatePhase.Blocked;
    public bool Offered => _snapshot.Phase == UpdatePhase.Offered;
    public bool Busy => _busy;
    public bool PeriodicChecks { get => _periodic; set => Set(ref _periodic, value); }
    public string Status => _snapshot.Phase == UpdatePhase.Blocked && _blockedReason?.Invoke() is { Length: > 0 } reason ? reason :
        _snapshot.Phase == UpdatePhase.Downloading ? UiText.Format("UpdateDownloading", _snapshot.Version, _snapshot.Percent) :
        _snapshot.Phase is UpdatePhase.Ready or UpdatePhase.Blocked or UpdatePhase.Offered ? UiText.Format("Update" + _snapshot.Phase, _snapshot.Version) : UiText.Get("Update" + _snapshot.Phase);
    public UpdateViewModel(IAppUpdater updater, Func<Task<bool>> restart, Func<string?>? blockedReason = null)
    {
        _updater = updater; _restart = restart; _blockedReason = blockedReason;
        _snapshot = new(updater.Available ? UpdatePhase.Idle : updater.Configuration.IsConfigured ? UpdatePhase.Portable : UpdatePhase.Unconfigured);
        PrimaryCommand = new(() => ActiveTask = Ready || Offered ? ApproveAsync() : CheckAsync(), () => !_stopping && !_busy && updater.Available);
        CheckCommand = new(() => ActiveTask = CheckAsync(), () => !_stopping && !_busy && updater.Available);
        CancelCommand = new(() => _cancel?.Cancel(), () => _busy && _snapshot.Phase != UpdatePhase.Installing);
        RestartCommand = new(() => ActiveTask = ApproveAsync(), () => !_stopping && !_busy && (Ready || Offered));
    }
    private void Publish(UpdateSnapshot snapshot)
    { _snapshot = snapshot; Changed(nameof(Status)); Changed(nameof(Notes)); Changed(nameof(Ready)); Changed(nameof(Offered)); Changed(nameof(PrimaryLabel)); Refresh(); }
    private void Refresh()
    { Changed(nameof(Busy)); Changed(nameof(CanSelectChannel)); PrimaryCommand?.Refresh(); CheckCommand?.Refresh(); CancelCommand?.Refresh(); RestartCommand?.Refresh(); }
    public async Task CheckAsync(CancellationToken token = default, bool download = false)
    {
        if (_stopping || _busy || !_updater.Available) return;
        _busy = true; Refresh();
        using var cancel = CancellationTokenSource.CreateLinkedTokenSource(token); cancel.CancelAfter(TimeSpan.FromMinutes(10)); _cancel = cancel;
        // No fire-and-forget progress callbacks: dispatch synchronously onto the captured UI context.
        var context = SynchronizationContext.Current;
        var progress = new InlineProgress<UpdateSnapshot>(value =>
        { if (context is null || SynchronizationContext.Current == context) Publish(value); else context.Send(_ => Publish(value), null); });
        try { await Task.Run(() => download ? _updater.DownloadApprovedAsync(progress, cancel.Token) : _updater.CheckAsync(progress, cancel.Token)); }
        catch (OperationCanceledException) { Publish(new(UpdatePhase.Cancelled)); }
        catch (Exception) { Publish(new(UpdatePhase.Failed)); }
        finally { _cancel = null; _busy = false; Refresh(); }
    }
    private async Task ApproveAsync()
    {
        if (Offered) await CheckAsync(download: true);
        if (!_stopping && Ready) await RestartAsync();
    }
    private async Task RestartAsync()
    {
        _busy = true; Refresh();
        var ready = _snapshot;
        try
        {
            Publish(ready with { Phase = UpdatePhase.Installing });
            if (!await _restart()) Publish(ready with { Phase = UpdatePhase.Blocked });
        }
        catch (Exception) { Publish(new(UpdatePhase.Failed)); }
        finally { _busy = false; Refresh(); }
    }
    public Task CheckOnOpenAsync()
    { _windowOpened = true; return ActiveTask = Ready || Busy ? ActiveTask : CheckAsync(); }
    public void Stop() { _stopping = true; _cancel?.Cancel(); Refresh(); }
    public void Resume() { _stopping = false; Refresh(); }
    public async Task CheckOnLaunchAsync(CancellationToken token)
    {
        if (_launchCheckStarted || _stopping || _busy || token.IsCancellationRequested || !_updater.Available) return;
        _launchCheckStarted = true;
        await CheckAsync(token);
    }
    public async Task RunAutomaticChecksAsync(CancellationToken token)
    {
        if (!_updater.Available) return;
        try
        {
            ActiveTask = CheckOnLaunchAsync(token); await ActiveTask;
            while (!token.IsCancellationRequested)
            {
                await Task.Delay(TimeSpan.FromHours(4) + TimeSpan.FromMinutes(Random.Shared.Next(30)), token);
                if (PeriodicChecks && !_stopping && !_busy && !Ready && !Offered) { ActiveTask = CheckAsync(); await ActiveTask; }
            }
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested) { }
    }
    private sealed class InlineProgress<T>(Action<T> report) : IProgress<T> { public void Report(T value) => report(value); }
}
