namespace GitGui.Core.Settings;

public enum AvatarProvider { Repository, Gravatar, Initials }

public sealed record AvatarSettings
{
    public AvatarProvider Provider { get; init; } = AvatarProvider.Repository;
    public string GiteaServer { get; init; } = "";
    public string? ValidationCode => !Enum.IsDefined(Provider) ? "SettingsAvatarInvalid" :
        !string.IsNullOrEmpty(GiteaServer) && !ValidServer(GiteaServer) ? "SettingsAvatarInvalid" : null;
    public AvatarSettings Normalize() => this with
    {
        Provider = Enum.IsDefined(Provider) ? Provider : AvatarProvider.Initials,
        GiteaServer = ValidServer(GiteaServer) ? GiteaServer.Trim().TrimEnd('/') : ""
    };
    public static bool ValidServer(string? value) => value is { Length: <= 2048 } && Uri.TryCreate(value, UriKind.Absolute, out var uri) &&
        uri.Scheme is "https" or "http" && uri.UserInfo.Length == 0 && uri.Query.Length == 0 && uri.Fragment.Length == 0;
}
