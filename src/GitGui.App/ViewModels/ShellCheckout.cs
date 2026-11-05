using GitGui.Core.Git;

namespace GitGui.App.ViewModels;

public sealed partial class ShellViewModel
{
    private ActionOptions? ReviewCheckout(RepositoryTab tab, ActionOptions options, string? subject = null)
    {
        if (!GitActions.IsCheckout(options)) return options;
        var detached = options.Action == GitAction.CheckoutCommit;
        var target = detached ? options.StartRef + (subject is null ? "" : " · " + subject) : options.Branch;
        var reference = tab.History.Page?.Session.References.FirstOrDefault(item =>
            item.Name == (options.Action == GitAction.SwitchBranch ? "refs/heads/" + options.Branch : options.StartRef));
        if (!detached && reference is not null) target += " · " + reference.Hash;
        using var review = new CheckoutViewModel(tab.Directory, target, detached,
            token => _actions!.ReadWorkingCopyAsync(_git!, tab.Directory, token));
        return _interaction!.ConfirmCheckout(review) ? review.Apply(options) with { CheckoutReference = detached ? null : reference, CheckoutStashMessage = Localization.UiText.Get("CheckoutStashMessage") } : null;
    }
}
