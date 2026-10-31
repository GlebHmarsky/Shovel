using Velopack.Logging;
namespace GitGui.App.Services;
// Updater exceptions/URLs are never persisted by Shovel.
public sealed class QuietUpdateLogger : IVelopackLogger
{
    public static QuietUpdateLogger Instance { get; } = new();
    public void Log(VelopackLogLevel logLevel, string? message, Exception? exception) { }
}
