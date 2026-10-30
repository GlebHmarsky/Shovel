using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Windows.Data;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed class ActivityEntry(object attempt, string repository, bool background, OperationActivity activity) : ObservableObject
{
    public object Attempt { get; } = attempt;
    public string Repository { get; } = repository;
    public bool Background { get; } = background;
    public DateTimeOffset Started { get; } = DateTimeOffset.Now;
    public string Time => Started.ToString("HH:mm:ss");
    private OperationActivity _activity = activity;
    public string Title => _activity.Title;
    public string Status => _activity.Phase;
    public string Summary => _activity.Help;
    public bool Running => !_activity.IsComplete;
    private string _output = "";
    private WeakReference<string>? _lastOutput;
    public string Output => _output;
    public void Update(OperationActivity value, string? output)
    {
        _activity = value;
        if (output is not null && !(_lastOutput?.TryGetTarget(out var prior) == true && ReferenceEquals(output, prior)))
        {
            _lastOutput = new(output);
            // User-visible, bounded session output only. Never written to diagnostic files/settings.
            var safe = GitActions.Redact(output);
            _output = safe.Length <= ActivityJournal.OutputLimit ? safe : safe[..ActivityJournal.OutputLimit] + "\n" + UiText.Get("ActivityOutputLimit");
            Changed(nameof(Output));
        }
        Changed(nameof(Title)); Changed(nameof(Status)); Changed(nameof(Summary)); Changed(nameof(Running));
    }
}

public sealed class ActivityJournal : ObservableObject
{
    public const int EntryLimit = 128, OutputLimit = 32768;
    public ObservableCollection<ActivityEntry> Entries { get; } = [];
    public ICollectionView Visible { get; }
    private ActivityEntry? _selected;
    public ActivityEntry? Selected { get => _selected; set => Set(ref _selected, value); }
    private string _repository = "";
    public string Repository { get => _repository; set { if (Set(ref _repository, value)) Refilter(); } }
    private int _filter;
    public int Filter { get => _filter; set { if (Set(ref _filter, Math.Clamp(value, 0, 2))) Refilter(); } }
    public ActivityJournal()
    {
        Visible = new ListCollectionView(Entries);
        Visible.Filter = item => item is ActivityEntry entry && entry.Repository.Equals(Repository, StringComparison.OrdinalIgnoreCase) &&
            (Filter == 0 || entry.Background == (Filter == 2));
    }
    public ActivityRead BeginRead(string repository, string title) => new(this, repository, title);
    private void Refilter()
    {
        Visible.Refresh();
        if (Selected is null || !Visible.Contains(Selected)) Selected = Visible.Cast<ActivityEntry>().FirstOrDefault();
    }
    public void Record(OperationActivity activity, string repository, bool background, string? output = null)
    {
        var entry = Entries.FirstOrDefault(e => ReferenceEquals(e.Attempt, activity.Attempt));
        if (entry is null)
        {
            entry = new(activity.Attempt, repository, background, activity);
            Entries.Insert(0, entry);
            while (Entries.Count > EntryLimit)
            {
                var oldest = Entries.LastOrDefault(e => !e.Running) ?? Entries[^1]; Entries.Remove(oldest);
                if (Selected == oldest) Selected = null;
            }
            if (Selected is null && Visible.Contains(entry)) Selected = entry;
        }
        entry.Update(activity, output);
    }
}

public sealed class ActivityRead : IDisposable
{
    private readonly ActivityJournal _journal;
    private readonly string _repository, _title;
    private readonly object _attempt = new();
    private readonly System.Diagnostics.Stopwatch _watch = System.Diagnostics.Stopwatch.StartNew();
    public OperationOutcome Outcome { get; set; } = OperationOutcome.Cancelled;
    public ActivityRead(ActivityJournal journal, string repository, string title)
    {
        _journal = journal; _repository = repository; _title = title;
        journal.Record(new(_attempt, title, UiText.Get("CommitFilesLoading"), 0, OperationOutcome.Running), repository, true);
    }
    public void Dispose() => _journal.Record(new(_attempt, _title, OperationActivity.CompletedPhase(Outcome), _watch.Elapsed.TotalSeconds, Outcome), _repository, true);
}
