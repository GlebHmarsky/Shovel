using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Documents;
using GitGui.App.Services;

namespace GitGui.FoundationChecks;

internal static partial class Program
{
    private static void RunThemeChecks()
    {
        Check("Dark surfaces retain readable text and system high-contrast mapping", () =>
        {
            var resources = Application.Current.Resources;
            try
            {
                AppTheme.Apply(resources, false);
                Color ColorOf(string name) => ((SolidColorBrush)resources["Theme." + name]).Color;
                foreach (var surface in new[] { "Chrome", "TabStrip", "Sidebar", "Canvas", "Panel", "Raised", "Input" })
                {
                    Expect(Contrast(ColorOf("Text"), ColorOf(surface)) >= 4.5, "Normal text contrast: " + surface);
                    Expect(Contrast(ColorOf("Muted"), ColorOf(surface)) >= 4.5, "Secondary text contrast: " + surface);
                }
                foreach (var surface in new[] { "Hover", "Pressed", "Selected", "ActiveSelection" })
                    Expect(Contrast(ColorOf("SelectionText"), ColorOf(surface)) >= 4.5, "State text contrast: " + surface);
                Expect(Contrast(ColorOf("Accent"), ColorOf("Sidebar")) >= 4.5 &&
                    Contrast(ColorOf("AuthorPeer"), ColorOf("Canvas")) >= 4.5, "Accent and author text remain readable");
                Expect(Contrast(ColorOf("DiffAddedText"), ColorOf("DiffAddedBackground")) >= 4.5 &&
                    Contrast(ColorOf("DiffRemovedText"), ColorOf("DiffRemovedBackground")) >= 4.5, "Added/removed text contrast");
                foreach (var marker in new[] { "FileModified", "FileAdded", "FileDeleted" })
                    Expect(Contrast(ColorOf(marker), ColorOf("Canvas")) >= 4.5, "File marker contrast: " + marker);
                var button = new Button { Content = "Existing control" };
                var host = new Border { Child = button };
                var window = new Window { Content = host };
                Arrange(host, 240, 50);
                Expect(((SolidColorBrush)button.Foreground).Color == ColorOf("Text"), "Control inherits the palette");
                AppTheme.Apply(resources, true); host.UpdateLayout();
                Expect(button.Foreground == SystemColors.WindowTextBrush && resources["Theme.ActiveSelection"] == SystemColors.HighlightBrush &&
                    resources["Theme.SelectionText"] == SystemColors.HighlightTextBrush && resources["Theme.Disabled"] == SystemColors.GrayTextBrush,
                    "High contrast follows Windows text, selection and disabled colors");
                Expect(resources["Theme.DiffAddedText"] == SystemColors.WindowTextBrush && resources["Theme.DiffRemovedBackground"] == SystemColors.WindowBrush,
                    "Diff colors yield to system contrast; line markers retain the relationship");
                foreach (var marker in new[] { "FileModified", "FileAdded", "FileDeleted" })
                    Expect(resources["Theme." + marker] == SystemColors.WindowTextBrush, "File markers yield to high contrast: " + marker);
                AppTheme.Apply(resources, false); host.UpdateLayout();
                Expect(((SolidColorBrush)button.Foreground).Color == ColorOf("Text"), "Existing controls return to dark colors without recreation");
                window.Close();
            }
            finally { AppTheme.Apply(resources, SystemParameters.HighContrast); }
            return Task.CompletedTask;
        }).GetAwaiter().GetResult();

        Check("Shared dark control templates preserve editable choices, selection and disabled states", () =>
        {
            var resources = Application.Current.Resources;
            var panel = new StackPanel { Margin = new Thickness(18) };
            var normal = new Button { Content = "Normal action", HorizontalAlignment = HorizontalAlignment.Left };
            var disabled = new Button { Content = "Unavailable action", IsEnabled = false, HorizontalAlignment = HorizontalAlignment.Left };
            panel.Children.Add(normal); panel.Children.Add(disabled);
            var tabs = new TabControl { Margin = new Thickness(0, 12, 0, 12) };
            tabs.Items.Add(new TabItem { Header = "Selected details", Content = new TextBlock { Text = "Details keep normal text", Height = 60, Margin = new Thickness(8) } });
            tabs.Items.Add(new TabItem { Header = "Other details" }); panel.Children.Add(tabs);
            var selectedTab = (TabItem)tabs.Items[0]; tabs.SelectedIndex = 0;
            var choices = new ComboBox { ItemsSource = new[] { "origin/main", "origin/feature" }, SelectedIndex = 0 };
            var editable = new ComboBox { IsEditable = true, Text = "feature/new", Margin = new Thickness(0, 8, 0, 8) };
            panel.Children.Add(choices); panel.Children.Add(editable);
            panel.Children.Add(new CheckBox { Content = "Enabled option", IsChecked = true, Margin = new Thickness(0, 8, 0, 8) });
            panel.Children.Add(new CheckBox { Content = "Disabled option", IsEnabled = false, IsChecked = true });
            panel.Children.Add(new RadioButton { Content = "Selected strategy", IsChecked = true, Margin = new Thickness(0, 8, 0, 8) });
            var grid = new DataGrid { AutoGenerateColumns = false, IsReadOnly = true, CanUserAddRows = false, HeadersVisibility = DataGridHeadersVisibility.None, Height = 110, Margin = new Thickness(0, 12, 0, 0) };
            grid.Columns.Add(new DataGridTextColumn { Binding = new System.Windows.Data.Binding(), Width = 470 });
            grid.ItemsSource = new[] { "Selected row", "Unselected row", "Another row" }; grid.SelectedIndex = 0; panel.Children.Add(grid);
            var host = new Border { Child = panel }; host.SetResourceReference(Border.BackgroundProperty, "Theme.Panel"); host.SetResourceReference(TextElement.ForegroundProperty, "Theme.Text");
            Arrange(host, 560, 520);
            Expect(disabled.Foreground == resources["Theme.Disabled"], "Disabled state overrides ordinary text");
            var header = (ContentPresenter)selectedTab.Template.FindName("Header", selectedTab);
            Expect(TextElement.GetForeground(header) == resources["Theme.Accent"] && selectedTab.FocusVisualStyle is not null, "Selected lower tab has accent and keyboard focus visual");
            selectedTab.IsEnabled = false;
            Expect(TextElement.GetForeground(header) == resources["Theme.Disabled"], "Disabled selected tab overrides its accent");
            selectedTab.IsEnabled = true;
            Expect(((TextBlock)selectedTab.Content).Foreground == resources["Theme.Text"], "Tab accent must not leak into details content");
            Expect(choices.SelectionBoxItem.ToString() == "origin/main", "Noneditable choice retains selected text");
            var editor = (TextBox)editable.Template.FindName("PART_EditableTextBox", editable);
            Expect(editor.Visibility == Visibility.Visible && editor.Text == "feature/new", "Editable branch uses WPF's named text input");
            editor.Text = "feature/edited";
            Expect(editable.Text == "feature/edited", "Template text editing still updates the ComboBox value");
            var row = (DataGridRow)grid.ItemContainerGenerator.ContainerFromIndex(0);
            Expect(row.IsSelected && row.Foreground == resources["Theme.SelectionText"], "Row selection keeps readable foreground");
            SaveRender(host, "theme-controls.png", 560, 520);
            // Rendering the menu itself does not open its popup or create a desktop surface.
            var menu = new ContextMenu();
            menu.Items.Add(new MenuItem { Header = "Refresh history" });
            menu.Items.Add(new MenuItem { Header = "Unavailable action", IsEnabled = false });
            menu.Items.Add(new Separator()); menu.Items.Add(new MenuItem { Header = "Close tab" });
            Arrange(menu, 250, 140);
            Expect(VisualDescendants<Border>(menu).First().Background == resources["Theme.Raised"], "Context menu surface uses the dark palette without the native light gutter");
            SaveRender(menu, "theme-menu.png", 250, 140);
            Expect(!Application.Current.Windows.OfType<Window>().Any(window => window.IsVisible), "Theme checks do not show desktop windows");
            return Task.CompletedTask;
        }).GetAwaiter().GetResult();
    }

    private static double Contrast(Color a, Color b)
    {
        static double Luminance(Color color)
        {
            static double Channel(byte value) { var x = value / 255d; return x <= 0.04045 ? x / 12.92 : Math.Pow((x + 0.055) / 1.055, 2.4); }
            return .2126 * Channel(color.R) + .7152 * Channel(color.G) + .0722 * Channel(color.B);
        }
        var first = Luminance(a); var second = Luminance(b);
        return (Math.Max(first, second) + .05) / (Math.Min(first, second) + .05);
    }
}
