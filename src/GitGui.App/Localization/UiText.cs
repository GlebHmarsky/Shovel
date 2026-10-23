using System.Globalization;
using System.Resources;
using System.Windows;
using System.Windows.Markup;

namespace GitGui.App.Localization;

public static class UiText
{
    private static readonly ResourceManager Resources = new("GitGui.App.Resources.Strings", typeof(UiText).Assembly);
    // English only for this release. Do not change CurrentCulture: numbers/dates use the user's locale.
    public static CultureInfo Culture { get; } = CultureInfo.GetCultureInfo("en");
    public static FlowDirection Direction => Culture.TextInfo.IsRightToLeft ? FlowDirection.RightToLeft : FlowDirection.LeftToRight;
    public static string Get(string key) => Resources.GetString(key, Culture) ?? throw new MissingManifestResourceException(key);
    public static string Format(string key, params object[] values) => string.Format(CultureInfo.CurrentCulture, Get(key), values);
}

[MarkupExtensionReturnType(typeof(string))]
public sealed class TextExtension : MarkupExtension
{
    public string Key { get; set; } = "";
    public override object ProvideValue(IServiceProvider serviceProvider) => UiText.Get(Key);
}
