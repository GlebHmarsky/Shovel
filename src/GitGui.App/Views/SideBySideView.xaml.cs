using System.Windows;
using System.Windows.Controls;
using GitGui.App.ViewModels;
namespace GitGui.App.Views;
public partial class SideBySideView : UserControl
{
    private long _generation;
    private bool _selecting;
    public static readonly DependencyProperty SelectedRowProperty = DependencyProperty.Register(nameof(SelectedRow), typeof(DiffRow), typeof(SideBySideView), new PropertyMetadata(null, (d, _) => ((SideBySideView)d).SelectRow()));
    public DiffRow? SelectedRow { get => (DiffRow?)GetValue(SelectedRowProperty); set => SetValue(SelectedRowProperty, value); }
    private void SelectRow()
    {
        if (_selecting || SelectedRow is null) return;
        var row = Table.Items.Cast<SideBySideRow>().FirstOrDefault(r => ReferenceEquals(r.Old, SelectedRow) || ReferenceEquals(r.New, SelectedRow));
        if (row is null) return;
        Table.SelectedItem = row; Table.ScrollIntoView(row);
    }
    public static readonly DependencyProperty RowsProperty = DependencyProperty.Register(nameof(Rows), typeof(IReadOnlyList<DiffRow>), typeof(SideBySideView), new PropertyMetadata(null, RowsChanged));
    public IReadOnlyList<DiffRow>? Rows { get => (IReadOnlyList<DiffRow>?)GetValue(RowsProperty); set => SetValue(RowsProperty, value); }
    public SideBySideView() { InitializeComponent(); }
    public Task Pending { get; private set; } = Task.CompletedTask;
    private static void RowsChanged(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        var view = (SideBySideView)sender;
        view.Pending = view.ProjectAsync((IReadOnlyList<DiffRow>?)args.NewValue ?? []);
    }
    private async Task ProjectAsync(IReadOnlyList<DiffRow> rows)
    {
        var generation = ++_generation;
        var paired = await Task.Run(() => SideBySideRow.Create(rows));
        if (generation == _generation) { Table.ItemsSource = paired; SelectRow(); }
    }
    private void OnSelection(object sender, SelectionChangedEventArgs args)
    {
        if (DataContext is CommitViewModel model)
        {
            _selecting = true;
            model.SelectedDiff = (Table.SelectedItem as SideBySideRow)?.Old ?? (Table.SelectedItem as SideBySideRow)?.New;
            _selecting = false;
            model.SelectDiffRows(Table.SelectedItems.Cast<SideBySideRow>().SelectMany(row => new[] { row.Old, row.New }).OfType<DiffRow>().Distinct().ToArray());
        }
    }
}
