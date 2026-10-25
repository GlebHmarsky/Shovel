using System.Globalization;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

/// <summary>Formatted once on the worker; no Git or parsing in cell bindings.</summary>
public sealed class CommitRow : ObservableObject
{
    public CommitRecord Commit { get; }
    public GraphRow Graph { get; }
    public string Description { get; }
    public string Author { get; }
    public string MessagePreview { get; }
    public string RefLabels { get; }
    public string RefPreview { get; }
    public string MessageNotice { get; }
    public string ShortHash { get; }
    public string Date { get; }
    public string AuthorDetails { get; }
    public string CommitterDetails { get; }
    public string Parents { get; }
    private readonly string _accessibilityLabel;
    private bool _isSameAuthor;
    public bool IsSameAuthor
    {
        get => _isSameAuthor;
        set { if (Set(ref _isSameAuthor, value)) Changed(nameof(AccessibilityLabel)); }
    }
    public string AccessibilityLabel => IsSameAuthor ? UiText.Format("SameAuthorAccessible", _accessibilityLabel) : _accessibilityLabel;
    // Exact raw author name + email, ordinal and case-sensitive; no mailmap or committer matching.
    // Missing/blank email never matches, including two incomplete identities.
    public bool HasSameAuthor(CommitRow? other) => other is not null && !string.IsNullOrWhiteSpace(Commit.AuthorEmail) &&
        string.Equals(Commit.Author, other.Commit.Author, StringComparison.Ordinal) &&
        string.Equals(Commit.AuthorEmail, other.Commit.AuthorEmail, StringComparison.Ordinal);

    public CommitRow(HistoryEntry entry)
    {
        Commit = entry.Commit; Graph = entry.Graph;
        var subject = Commit.Subject;
        Description = subject.Length == 0 ? UiText.Get("EmptyCommitSubject") : Limit(subject, 512);
        Author = Limit(Commit.Author, 256);
        MessagePreview = Commit.Message;
        MessageNotice = "";
        RefLabels = string.Join(UiText.Get("RefSeparator"), (entry.References ?? []).Select(reference => reference.Name == "HEAD" ? UiText.Get("HeadLabel") :
            UiText.Format(reference.Name.StartsWith("refs/heads/", StringComparison.Ordinal) ? "LocalRefLabel" :
                reference.Name.StartsWith("refs/remotes/", StringComparison.Ordinal) ? "RemoteRefLabel" : "TagRefLabel",
                reference.Name[(reference.Name.IndexOf('/', 5) + 1)..])));
        RefPreview = Limit(RefLabels, 512);
        ShortHash = Commit.Hash[..Math.Min(12, Commit.Hash.Length)];
        Date = Commit.CommittedAt.LocalDateTime.ToString("g", CultureInfo.CurrentCulture);
        AuthorDetails = UiText.Format("CommitIdentity", Commit.Author, Commit.AuthorEmail, Commit.AuthoredAt.ToString("G", CultureInfo.CurrentCulture), Commit.AuthoredAt.ToString("zzz", CultureInfo.InvariantCulture));
        CommitterDetails = UiText.Format("CommitIdentity", Commit.Committer, Commit.CommitterEmail, Commit.CommittedAt.ToString("G", CultureInfo.CurrentCulture), Commit.CommittedAt.ToString("zzz", CultureInfo.InvariantCulture));
        Parents = Commit.Parents.Count == 0 ? UiText.Get("RootCommit") : string.Join(Environment.NewLine, Commit.Parents);
        _accessibilityLabel = UiText.Format("CommitRowAccessible", Description, Author, ShortHash, Parents, RefLabels);
    }
    private static string Limit(string value, int length) => value.Length > length ? UiText.Format("TruncatedCommitText", value[..length]) : value;
}
