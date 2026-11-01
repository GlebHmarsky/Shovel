using System.Windows;
using GitGui.App.Services;
using GitGui.App.ViewModels;
namespace GitGui.App.Views;
public partial class UpdateWindow : Window
{
    public UpdateWindow(UpdateViewModel model) { InitializeComponent(); DataContext = model; TitleBarTheme.Attach(this); Loaded += async (_, _) => { await model.CheckOnOpenAsync(); }; }
}
