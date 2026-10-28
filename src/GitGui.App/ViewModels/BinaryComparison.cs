using System.IO;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed record BinarySide(bool IsOld, string Extension, long? Bytes, bool IsLfs, bool Missing)
{
    public string Label => UiText.Get(IsOld ? "BinaryOld" : "BinaryNew");
    public string Size => Missing ? UiText.Get("BinaryAbsent") : Bytes is not { } bytes ? UiText.Get("CountUnavailable") : FormatBytes(bytes);
    public string Accessible => UiText.Format("BinarySideAccessible", Label, Extension, Size, IsLfs ? UiText.Get("BinaryLfs") : UiText.Get("BinaryStoredFile"));
    private static string FormatBytes(long bytes)
    {
        string[] units = ["BinaryBytes", "BinaryKiB", "BinaryMiB", "BinaryGiB", "BinaryTiB"];
        var unit = 0; double amount = bytes;
        while (amount >= 1024 && unit < units.Length - 1) { amount /= 1024; unit++; }
        return unit == 0 ? UiText.Format("BinaryExactBytes", bytes) : UiText.Format("BinarySize", amount, UiText.Get(units[unit]), bytes);
    }
}
public sealed record BinaryComparison(IReadOnlyList<BinarySide> Sides)
{
    public static BinaryComparison? Create(FileDiff diff, LocalChange file)
    {
        if (diff.Kind != FileDiffKind.Binary) return null;
        string Extension(string path) => Path.GetExtension(path) is { Length: > 0 } extension ? extension : UiText.Get("BinaryNoExtension");
        return new([new(true, Extension(file.OriginalPath ?? file.Path), diff.OldBytes, diff.OldLfs, file.Code is 'A' or '?'),
            new(false, Extension(file.Path), diff.NewBytes, diff.NewLfs, file.Code == 'D')]);
    }
}
