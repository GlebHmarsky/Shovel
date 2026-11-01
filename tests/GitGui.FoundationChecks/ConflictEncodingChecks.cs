using System.IO;
using System.Text;
using System.Windows;
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
    private static async Task RunConflictEncodingChecksAsync()
    {
        var runner = new HistoryActionRunner(); var reader = new RepositoryReader(runner);
        var actions = new GitActions(runner, reader, new WindowsRepositoryIdentity()); var git = await reader.DiscoverAsync(null, default);
        var little = new UnicodeEncoding(false, true, true); var big = new UnicodeEncoding(true, true, true); var utf8 = new UTF8Encoding(true, true);
        byte[] Bytes(string text, Encoding encoding) => [.. encoding.GetPreamble(), .. encoding.GetBytes(text)];
        async Task<byte[]> Git(string repo, params string[] args)
        {
            var result = await runner.RunAsync(new(git.Executable, repo, args) { MaximumOutputBytes = 1024 * 1024 }, default);
            Expect(result.Outcome == ProcessOutcome.Exited && result.ExitCode == 0,
                result.Outcome + ": " + Encoding.UTF8.GetString(result.StandardError)); return result.StandardOutput;
        }
        async Task<string> Conflict(string name, Encoding current, Encoding incoming)
        {
            var repo = Fixture("conflict encoding " + name); await Git(repo, "init", "-b", "main");
            await Git(repo, "config", "user.name", "Encoding fixture"); await Git(repo, "config", "user.email", "encoding@example.invalid");
            await Git(repo, "config", "core.autocrlf", "false");
            await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), Bytes("base\r\n", current));
            await Git(repo, "add", "."); await Git(repo, "commit", "-m", "base"); await Git(repo, "switch", "-c", "incoming");
            await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), Bytes("incoming Ω\r\n", incoming));
            await Git(repo, "commit", "-am", "incoming"); await Git(repo, "switch", "main");
            await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), Bytes("current 日本語\r\n", current));
            await Git(repo, "commit", "-am", "current");
            var merge = await runner.RunAsync(new(git.Executable, repo, ["merge", "--no-edit", "incoming"]), default);
            Expect(merge.ExitCode != 0, "Fixture must be unmerged"); return repo;
        }
        async Task<ConflictPreview> Preview(string repo)
        {
            var context = await actions.ReadWorkingCopyAsync(git, repo, default);
            return await actions.ReadConflictAsync(context, context.Changes.Entries.Single(e => e.Group == ChangeGroup.Conflicted), default);
        }
        Task<WorkingCopyResult> Resolve(ConflictPreview preview, string text) => actions.ExecuteWorkingCopyAsync(preview.Context,
            new(WorkingCopyAction.ResolveConflict, [preview.Change], Resolution: new(preview, ConflictChoice.Edited, text)), default);
        async Task Reject(ConflictPreview preview, string text, string code)
        {
            try { await Resolve(preview, text); throw new InvalidOperationException("Expected rejection: " + code); }
            catch (GitActionException error) { Expect(error.Code == code, "Expected " + code + "; got " + error.Code); }
        }

        await Check("Inline UTF-16 editor previews mixed byte order and stages exact BOM, Unicode and CRLF", async () =>
        {
            var repo = await Conflict("mixed", little, big); var preview = await Preview(repo);
            Expect(preview.CanEdit && preview.TextEncoding == ConflictTextEncoding.Utf16LittleEndianBom &&
                preview.Incoming?.TextEncoding == ConflictTextEncoding.Utf16BigEndianBom, "Both source encodings are detected");
            var model = new ConflictViewModel(preview.Context, preview.Change, actions);
            const string result = "\ufeffMerged 日本語 Ω 😀\r\nSecond line\r\n";
            try
            {
                await model.StartAsync(); var calls = runner.Calls;
                Expect(model.ResultLabel.Contains("UTF-16 LE"), "Saved result encoding shown");
                model.Current = false;
                Expect(model.Text == "incoming Ω\r\n" && model.ResultLabel.Contains("UTF-16 BE"), "Native whole-side preview has its own encoding");
                model.Text = result;
                Expect(model.Current && model.Incoming && model.ResultLabel.Contains("UTF-16 LE") && runner.Calls == calls, "Typing restores combined result encoding without Git work");
                foreach (var theme in new[] { AppAppearance.Dark, AppAppearance.Light })
                {
                    AppTheme.Apply(Application.Current.Resources, false, theme);
                    // Unshown controls are not in Application.Windows for theme invalidation.
                    var view = new ConflictView { DataContext = model };
                    foreach (var width in new[] { 1100, 800 }) { Arrange(view, width, 570); SaveRender(view, $"conflict-utf16-{theme}-{width}.png", width, 570); }
                }
                model.ResolveCommand.Execute(null); await model.Work;
                var expected = Bytes(result, little);
                Expect(model.Resolved && (await File.ReadAllBytesAsync(Path.Combine(repo, "file.txt"))).SequenceEqual(expected) &&
                    (await Git(repo, "show", ":file.txt")).SequenceEqual(expected), "Worktree and actual staged blob preserve exact encoding and content");
            }
            finally { await model.CloseAsync(); AppTheme.Apply(Application.Current.Resources, false); }
        });
        await Check("UTF-16 big-endian edited resolution preserves byte order in the index", async () =>
        {
            var repo = await Conflict("big endian", big, big); var p = await Preview(repo); const string result = "Resolved 😀\n";
            Expect(p.CanEdit && p.TextEncoding == ConflictTextEncoding.Utf16BigEndianBom, "Big-endian editor available");
            Expect((await Resolve(p, result)).Result.Success && (await Git(repo, "show", ":file.txt")).SequenceEqual(Bytes(result, big)), "Exact big-endian staged bytes");
        });
        await Check("UTF-8 BOM resolution retains a leading content U+FEFF after the transport BOM", async () =>
        {
            var repo = await Conflict("utf8 bom", utf8, utf8);
            const string content = "\ufeffcontent\n"; await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), Bytes(content, utf8));
            var p = await Preview(repo);
            Expect(p.CanEdit && p.WorkingText == content && p.TextEncoding == ConflictTextEncoding.Utf8Bom, "Only one transport BOM removed");
            Expect((await Resolve(p, content)).Result.Success && (await Git(repo, "show", ":file.txt")).SequenceEqual(Bytes(content, utf8)), "Both BOM and leading content preserved");
        });
        await Check("Malformed or BOM-less UTF-16 conflict text stays in the native side/tool workflow", async () =>
        {
            var repo = await Conflict("invalid", little, big);
            byte[][] invalid = [[0xff, 0xfe, 0x41], [0xff, 0xfe, 0, 0xd8], Encoding.Unicode.GetBytes("No BOM")];
            foreach (var bytes in invalid)
            {
                await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), bytes); var p = await Preview(repo);
                Expect(!p.CanEdit && p.WorkingText is null, "Invalid input cannot be decoded with replacement or saved as text");
            }
        });
        await Check("UTF-32, binary, LFS pointer and filtered content remain outside conflict text editing", async () =>
        {
            var repo = await Conflict("unsupported", little, big);
            byte[][] unsupported = [[0xff, 0xfe, 0, 0, 0x41, 0, 0, 0], Bytes("embedded\0null", little),
                Bytes("version https://git-lfs.github.com/spec/v1\n", big)];
            foreach (var bytes in unsupported)
            {
                await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), bytes);
                Expect(!(await Preview(repo)).CanEdit, "Unsupported content cannot authorize a text write");
            }
            await File.WriteAllBytesAsync(Path.Combine(repo, "file.txt"), Bytes("valid\n", little));
            await File.WriteAllTextAsync(Path.Combine(repo, ".gitattributes"), "file.txt filter=custom\n");
            Expect(!(await Preview(repo)).CanEdit, "Custom transformation still requires native resolution");
        });
        await Check("UTF-16 edit limits count saved bytes including BOM and reject malformed Unicode and markers", async () =>
        {
            var repo = await Conflict("bounds", little, little); var p = await Preview(repo);
            var content = string.Concat(Enumerable.Repeat(new string('x', 1023) + "\n", 255)) + new string('y', 1023);
            Expect(Bytes(content, little).Length == GitActions.MaximumConflictTextBytes, "Exact encoded byte boundary");
            await Reject(p, content + "x", "ConflictTextLimit");
            await Reject(p, "invalid \ud800", "ConflictTextLimit");
            await Reject(p, "<<<<<<< current\r\nmarkers\r\n=======\r\n>>>>>>> incoming\r\n", "ConflictMarkersRemain");
            Expect((await Resolve(p, content)).Result.Success && (await Git(repo, "show", ":file.txt")).SequenceEqual(Bytes(content, little)), "Boundary-sized valid text stages exactly");
        });
        await Check("Failed staging retains the edited UTF-16 file with its BOM and foreign lock", async () =>
        {
            var repo = await Conflict("failed add", big, big); var p = await Preview(repo); var file = Path.Combine(repo, "file.txt");
            await File.WriteAllTextAsync(Path.Combine(repo, ".git", "index.lock"), "fixture lock"); const string edited = "Keep edited Ω\r\n";
            var result = await Resolve(p, edited);
            Expect(!result.Result.Success && (await File.ReadAllBytesAsync(file)).SequenceEqual(Bytes(edited, big)) &&
                File.Exists(Path.Combine(repo, ".git", "index.lock")), "Failed Git add preserves actual user result and existing lock");
        });
    }
}
