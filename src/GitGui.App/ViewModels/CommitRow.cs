using System.Globalization;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

/// <summary>Formatted once on the worker; no Git or parsing in cell bindings.</summary>
public sealed class CommitRow
{
    public CommitRecord Commit { get; }
    public GraphRow Graph { get; }
    public string Description { get; }
    public string Author { get; }
    public string MessagePreview { get; }
    public string MessageNotice { get; }
    public string ShortHash { get; }
    public string Date { get; }
    public string AuthorDetails { get; }
    public string CommitterDetails { get; }
    public string Parents { get; }
    public string AccessibilityLabel { get; }

    public CommitRow(HistoryEntry entry)
    {
        Commit = entry.Commit; Graph = entry.Graph;
        var subject = Commit.Subject;
        Description = subject.Length == 0 ? UiText.Get("EmptyCommitSubject") : Limit(subject, 512);
        Author = Limit(Commit.Author, 256);
        MessagePreview = Commit.Message.Length > 64 * 1024 ? Commit.Message[..(64 * 1024)] : Commit.Message;
        MessageNotice = Commit.Message.Length > MessagePreview.Length ? UiText.Get("CommitMessageTruncated") : "";
        ShortHash = Commit.Hash[..Math.Min(12, Commit.Hash.Length)];
        Date = Commit.CommittedAt.LocalDateTime.ToString("g", CultureInfo.CurrentCulture);
        AuthorDetails = UiText.Format("CommitIdentity", Author, Limit(Commit.AuthorEmail, 512), Commit.AuthoredAt.ToString("G", CultureInfo.CurrentCulture), Commit.AuthoredAt.ToString("zzz", CultureInfo.InvariantCulture));
        CommitterDetails = UiText.Format("CommitIdentity", Limit(Commit.Committer, 256), Limit(Commit.CommitterEmail, 512), Commit.CommittedAt.ToString("G", CultureInfo.CurrentCulture), Commit.CommittedAt.ToString("zzz", CultureInfo.InvariantCulture));
        Parents = Commit.Parents.Count == 0 ? UiText.Get("RootCommit") : string.Join(Environment.NewLine, Commit.Parents);
        AccessibilityLabel = UiText.Format("CommitRowAccessible", Description, Author, ShortHash, Parents);
    }
    private static string Limit(string value, int length) => value.Length > length ? UiText.Format("TruncatedCommitText", value[..length]) : value;
}
