using System.Windows;
using System.Windows.Controls;
using System.Windows.Media.Animation;
using GitGui.App.ViewModels;

namespace GitGui.App.Views;

public partial class OperationIndicator : UserControl
{
    private AnimationClock? _completionClock;
    public static readonly DependencyProperty ActivityProperty = DependencyProperty.Register(nameof(Activity), typeof(OperationActivity), typeof(OperationIndicator), new PropertyMetadata(null, OnActivityChanged));
    public static readonly DependencyProperty CaptionProperty = DependencyProperty.Register(nameof(Caption), typeof(string), typeof(OperationIndicator), new PropertyMetadata(""));
    public OperationActivity? Activity { get => (OperationActivity?)GetValue(ActivityProperty); set => SetValue(ActivityProperty, value); }
    public string Caption { get => (string)GetValue(CaptionProperty); set => SetValue(CaptionProperty, value); }
    public OperationIndicator()
    {
        InitializeComponent(); DurationBadge.Opacity = 0;
        Unloaded += (_, _) => { StopCompletion(); DurationBadge.Opacity = Activity is { IsComplete: false } ? 1 : 0; };
    }
    private void StopCompletion()
    {
        _completionClock?.Controller?.Remove(); _completionClock = null;
        DurationBadge.ApplyAnimationClock(OpacityProperty, null);
    }
    private static void OnActivityChanged(DependencyObject sender, DependencyPropertyChangedEventArgs e)
    {
        var view = (OperationIndicator)sender;
        var previous = (OperationActivity?)e.OldValue; var current = (OperationActivity?)e.NewValue;
        // A timer/refresh notification must not replay an already completed attempt.
        if (current is { IsComplete: true } && previous is { IsComplete: true } && ReferenceEquals(current.Attempt, previous.Attempt)) return;
        view.StopCompletion();
        view.DurationBadge.Opacity = current is null ? 0 : 1;
        if (current is not { IsComplete: true }) return;
        // Honor Windows animation preference and high contrast without changing either.
        var duration = SystemParameters.ClientAreaAnimation && !SystemParameters.HighContrast ? TimeSpan.FromSeconds(2) : TimeSpan.Zero;
        view._completionClock = new DoubleAnimation(1, 0, duration)
        { BeginTime = TimeSpan.FromSeconds(3), FillBehavior = FillBehavior.HoldEnd }.CreateClock();
        view.DurationBadge.ApplyAnimationClock(OpacityProperty, view._completionClock);
    }
}
