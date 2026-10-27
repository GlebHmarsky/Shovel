using System.Runtime.InteropServices;
using System.Text;
using System.Windows;
using GitGui.App.Localization;
using GitGui.Core.Git;

namespace GitGui.App.Views;

public partial class AuthenticationWindow : Window
{
    private readonly AuthenticationPromptKind _kind;
    private AuthenticationAnswer? _answer;
    public AuthenticationWindow(string context, AuthenticationPrompt prompt)
    {
        InitializeComponent(); _kind = prompt.Kind;
        ContextText.Text = context; PromptText.Text = GitActions.Redact(prompt.Message);
        ResponsePanel.Visibility = _kind == AuthenticationPromptKind.Secret ? Visibility.Visible : Visibility.Collapsed;
        Submit.Content = UiText.Get(_kind == AuthenticationPromptKind.Confirmation ? "AuthApprove" : "AuthSubmit");
        Submit.Visibility = _kind == AuthenticationPromptKind.Information ? Visibility.Collapsed : Visibility.Visible;
        Loaded += (_, _) => { if (_kind == AuthenticationPromptKind.Secret) Response.Focus(); else CancelPrompt.Focus(); };
    }
    public AuthenticationAnswer? TakeAnswer() { var answer = _answer; _answer = null; return answer; }
    private void OnSubmit(object sender, RoutedEventArgs e)
    {
        if (_kind == AuthenticationPromptKind.Confirmation) _answer = new("yes"u8.ToArray());
        else
        {
            using var secure = Response.SecurePassword;
            var pointer = Marshal.SecureStringToBSTR(secure);
            var chars = new char[secure.Length];
            try { Marshal.Copy(pointer, chars, 0, chars.Length); _answer = new(Encoding.UTF8.GetBytes(chars)); }
            finally { Array.Clear(chars); Marshal.ZeroFreeBSTR(pointer); }
        }
        Close();
    }
    private void OnCancel(object sender, RoutedEventArgs e) => Close();
    private void OnClosed(object? sender, EventArgs e) => Response.Clear();
}
