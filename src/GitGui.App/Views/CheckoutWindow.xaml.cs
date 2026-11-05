using System.Windows;
using GitGui.App.Services;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class CheckoutWindow : Window
{
    private readonly CheckoutViewModel _model;
    public CheckoutWindow(CheckoutViewModel model)
    {
        _model = model; InitializeComponent(); DataContext = model; TitleBarTheme.Attach(this);
        Loaded += async (_, _) => await model.StartAsync();
    }
    private void OnConfirm(object sender, RoutedEventArgs e) { if (_model.CanConfirm) DialogResult = true; }
}
