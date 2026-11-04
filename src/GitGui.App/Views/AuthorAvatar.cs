using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using GitGui.App.Services;
using GitGui.App.ViewModels;
using GitGui.Core.Settings;

namespace GitGui.App.Views;

public sealed class AuthorAvatar : Border
{
    public static readonly DependencyProperty AuthorProperty = DependencyProperty.Register(nameof(Author), typeof(CommitRow), typeof(AuthorAvatar), new PropertyMetadata(null, Reload));
    public static readonly DependencyProperty ServiceProperty = DependencyProperty.Register(nameof(Service), typeof(AvatarService), typeof(AuthorAvatar), new PropertyMetadata(null, Reload));
    public static readonly DependencyProperty PreferencesProperty = DependencyProperty.Register(nameof(Preferences), typeof(AvatarSettings), typeof(AuthorAvatar), new PropertyMetadata(null, Reload));
    public static readonly DependencyProperty RemoteProperty = DependencyProperty.Register(nameof(Remote), typeof(string), typeof(AuthorAvatar), new PropertyMetadata("", Reload));
    public CommitRow? Author { get => (CommitRow?)GetValue(AuthorProperty); set => SetValue(AuthorProperty, value); }
    public AvatarService? Service { get => (AvatarService?)GetValue(ServiceProperty); set => SetValue(ServiceProperty, value); }
    public AvatarSettings? Preferences { get => (AvatarSettings?)GetValue(PreferencesProperty); set => SetValue(PreferencesProperty, value); }
    public string Remote { get => (string)GetValue(RemoteProperty); set => SetValue(RemoteProperty, value); }
    private CancellationTokenSource? _cancel;
    private readonly TextBlock _initial = new() { HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center, Foreground = Brushes.White, FontWeight = FontWeights.SemiBold };
    private readonly Image _image = new() { Stretch = Stretch.UniformToFill };
    private readonly Grid _content = new();
    public AuthorAvatar()
    {
        CornerRadius = new(4); ClipToBounds = true; _content.Children.Add(_initial); _content.Children.Add(_image); Child = _content;
        Loaded += (_, _) => Refresh(); Unloaded += (_, _) => { _cancel?.Cancel(); _cancel = null; };
        SizeChanged += (_, _) => _initial.FontSize = Math.Max(10, ActualHeight * .46);
    }
    private static void Reload(DependencyObject target, DependencyPropertyChangedEventArgs args) => ((AuthorAvatar)target).Refresh();
    private async void Refresh()
    {
        _cancel?.Cancel(); _cancel = null; _image.Source = null;
        var row = Author; if (row is null) { _initial.Text = ""; Background = Brushes.Transparent; return; }
        var name = row.Commit.Author.Trim(); _initial.Text = name.Length == 0 ? "?" : StringInfo.GetNextTextElement(name).ToUpperInvariant();
        var color = SHA256.HashData(Encoding.UTF8.GetBytes(row.Commit.AuthorEmail.Trim().ToLowerInvariant() is { Length: > 0 } email ? email : name));
        Background = SystemParameters.HighContrast ? SystemColors.HighlightBrush : new SolidColorBrush(Color.FromRgb((byte)(45 + color[0] % 65), (byte)(45 + color[1] % 65), (byte)(45 + color[2] % 65)));
        _initial.Foreground = SystemParameters.HighContrast ? SystemColors.HighlightTextBrush : Brushes.White;
        ToolTip = row.AuthorDetails;
        if (!IsLoaded || Service is null || Preferences is null) return;
        var cancel = new CancellationTokenSource(); _cancel = cancel;
        try
        {
            var image = await Service.GetAsync(Preferences, Remote, row.Commit.Hash, row.Commit.AuthorEmail, cancel.Token);
            if (!cancel.IsCancellationRequested && ReferenceEquals(_cancel, cancel)) _image.Source = image;
        }
        catch (OperationCanceledException) { }
        finally { if (ReferenceEquals(_cancel, cancel)) _cancel = null; cancel.Dispose(); }
    }
}
