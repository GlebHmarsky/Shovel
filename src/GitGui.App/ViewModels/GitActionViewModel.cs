using System.Diagnostics;
using System.Text;
using System.Windows.Threading;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class GitActionViewModel : ObservableObject
{
    private readonly GitActions _actions;
    private readonly Func<Task> _reconcile;
    private readonly CancellationTokenSource _cancel = new();
    private readonly Stopwatch _elapsed = new();
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromMilliseconds(250) };
    private readonly List<byte> _output = [];
    private int _publishedBytes;
    private Task _outputTask = Task.CompletedTask;
    public ActionContext Context { get; }
    public GitAction Action { get; }
    public bool IsNew => Action == GitAction.NewBranch;
    public bool IsSwitch => Action == GitAction.SwitchBranch;
    public bool IsFetch => Action == GitAction.Fetch;
    public bool IsPull => Action == GitAction.Pull;
    public bool IsPush => Action == GitAction.Push;
    public bool IsNetwork => IsFetch || IsPull || IsPush;
    public bool ChooseBranch => IsPush || IsSwitch;
    public bool ChooseRemoteBranch => IsPush || IsPull;
    public string Title => UiText.Get("ActionTitle" + Action);
    public string Explanation => UiText.Get("ActionExplain" + Action);
    public string Into => Context.Repository.Branch ?? UiText.Get("NoBranch");
    public string CurrentRepository => Context.Repository.Worktree;
    public IReadOnlyList<GitRemote> Remotes => Context.Remotes;
    public IReadOnlyList<string> Branches { get; }
    public IReadOnlyList<string> StartRefs { get; }
    public string[] RemoteBranches => Context.History.References.Where(item => item.Name.StartsWith("refs/remotes/" + Remote?.Name + "/", StringComparison.Ordinal))
        .Select(item => item.Name[(14 + Remote!.Name.Length)..]).ToArray();
    private GitRemote? _remote;
    public GitRemote? Remote { get => _remote; set { if (Set(ref _remote, value)) { Changed(nameof(Target)); Changed(nameof(RemoteBranches)); Refresh(); } } }
    public string Target => Remote is null ? UiText.Get("ActionNoRemote") : IsPush ? Remote.PushAddress : Remote.FetchAddress;
    private string _branch = "", _remoteBranch = "", _newName = "", _startRef = "HEAD";
    public string Branch { get => _branch; set { if (Set(ref _branch, value)) Refresh(); } }
    public string RemoteBranch { get => _remoteBranch; set { if (Set(ref _remoteBranch, value)) Refresh(); } }
    public string NewName { get => _newName; set { if (Set(ref _newName, value)) Refresh(); } }
    public string StartRef { get => _startRef; set { if (Set(ref _startRef, value)) Refresh(); } }
    private bool _checkout = true, _allRemotes, _allTags, _setUpstream, _approved, _running, _attempted;
    public bool Checkout { get => _checkout; set => Set(ref _checkout, value); }
    public bool AllRemotes { get => _allRemotes; set => Set(ref _allRemotes, value); }
    public bool AllTags { get => _allTags; set => Set(ref _allTags, value); }
    public bool SetUpstream { get => _setUpstream; set => Set(ref _setUpstream, value); }
    public bool Approved { get => _approved; set { if (Set(ref _approved, value)) Refresh(); } }
    public bool IsRunning { get => _running; private set { if (Set(ref _running, value)) { Changed(nameof(CanEdit)); Changed(nameof(CloseLabel)); Refresh(); } } }
    public bool CanEdit => !IsRunning && !_attempted;
    public bool HasAttempt => _attempted;
    public string CloseLabel => UiText.Get(IsRunning ? "ActionCancelRunning" : HasAttempt ? "ActionClose" : "ActionCancel");
    private string _status = UiText.Get("ActionReady"), _rawOutput = "";
    public string Status { get => _status; private set => Set(ref _status, value); }
    public string RawOutput { get => _rawOutput; private set => Set(ref _rawOutput, value); }
    public string Duration => UiText.Format("ActionDuration", _elapsed.Elapsed.TotalSeconds);
    public RelayCommand RunCommand { get; }
    public Task RunningTask { get; private set; } = Task.CompletedTask;

    public GitActionViewModel(ActionContext context, GitAction action, GitActions actions, Func<Task> reconcile, string? initialBranch = null)
    {
        Context = context; Action = action; _actions = actions; _reconcile = reconcile;
        Branches = context.History.References.Where(item => item.Name.StartsWith("refs/heads/", StringComparison.Ordinal)).Select(item => item.Name[11..]).ToArray();
        StartRefs = context.History.References.Select(item => item.Name).OrderBy(item => item == "HEAD" ? "" : item, StringComparer.Ordinal).ToArray();
        _branch = initialBranch ?? context.Repository.Branch ?? Branches.FirstOrDefault() ?? "";
        _startRef = StartRefs.FirstOrDefault() ?? "";
        _remote = Remotes.FirstOrDefault(item => context.Repository.UpstreamRef?.StartsWith("refs/remotes/" + item.Name + "/", StringComparison.Ordinal) == true) ?? Remotes.FirstOrDefault();
        _remoteBranch = context.Repository.UpstreamRef is { } upstream && _remote is not null && upstream.StartsWith("refs/remotes/" + _remote.Name + "/", StringComparison.Ordinal)
            ? upstream[(14 + _remote.Name.Length)..] : _branch;
        RunCommand = new(() => RunningTask = RunAsync(), () => CanEdit && Approved && (!IsNetwork || Remote is not null) &&
            (!IsNew || !string.IsNullOrWhiteSpace(NewName) && StartRefs.Contains(StartRef)) &&
            (!ChooseBranch || Branches.Contains(Branch)) && (!ChooseRemoteBranch || !string.IsNullOrWhiteSpace(RemoteBranch)));
        _timer.Tick += (_, _) => { Changed(nameof(Duration)); if (_outputTask.IsCompleted) _outputTask = PublishOutputAsync(); };
    }
    private void Refresh() => RunCommand?.Refresh();
    public void Cancel() { if (IsRunning) { Status = UiText.Get("ActionCancelling"); _cancel.Cancel(); } }
    private async Task PublishOutputAsync()
    {
        var text = await Task.Run(() =>
        {
            byte[] bytes;
            lock (_output)
            {
                if (_publishedBytes == _output.Count) return null;
                _publishedBytes = _output.Count; bytes = _output.ToArray();
            }
            return GitActions.Redact(Encoding.UTF8.GetString(bytes));
        });
        if (text is not null) RawOutput = text;
    }
    private async Task RunAsync()
    {
        _attempted = true; Changed(nameof(HasAttempt)); IsRunning = true; _elapsed.Restart(); _timer.Start(); Status = UiText.Get("ActionRunning");
        try
        {
            var result = await _actions.ExecuteAsync(Context, new(Action, IsNew ? NewName : Branch, StartRef,
                Remote?.Name ?? "", RemoteBranch, Checkout, AllRemotes, AllTags, SetUpstream, Approved), _cancel.Token, output =>
                { lock (_output) { if (_output.Count + output.Bytes.Length <= 1024 * 1024) _output.AddRange(output.Bytes); } });
            _timer.Stop(); await _outputTask; RawOutput = result.Output;
            Status = UiText.Get(result.Success ? "ActionSucceeded" : "ActionResult" + result.Outcome);
        }
        catch (GitActionException exception) { _timer.Stop(); await _outputTask; await PublishOutputAsync(); Status = UiText.Get(exception.Code); }
        catch (GitReadException exception) { _timer.Stop(); await _outputTask; Status = UiText.Get(exception.Failure == GitFailure.Cancelled ? "ActionResultCancelled" : "ActionReadFailed"); RawOutput = exception.RawOutput; }
        catch (OperationCanceledException) { _timer.Stop(); await _outputTask; await PublishOutputAsync(); Status = UiText.Get("ActionResultCancelled"); }
        catch (Exception exception) when (exception is System.IO.IOException or UnauthorizedAccessException or System.ComponentModel.Win32Exception)
        { Status = UiText.Get("ActionReadFailed"); }
        finally
        {
            _timer.Stop(); _elapsed.Stop(); Changed(nameof(Duration));
            await _outputTask;
            // Cancellation is not rollback. Reinspection uses a fresh cancellation scope.
            try { await _reconcile(); }
            finally { IsRunning = false; _cancel.Dispose(); }
        }
    }
}
