using System.IO;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Windows.Media.Imaging;
using GitGui.Core.Settings;

namespace GitGui.App.Services;

/// <summary>Optional, bounded image reads. No Git credentials, cookies or persistent author data.</summary>
public sealed class AvatarService : IDisposable
{
    private readonly HttpClient _http;
    private readonly SemaphoreSlim _slots = new(2, 2);
    private CancellationTokenSource _stop = new();
    private bool _disposed;
    private readonly Dictionary<string, (Task<BitmapSource?> Task, DateTimeOffset At)> _cache = [];
    public AvatarService(HttpMessageHandler? handler = null)
    {
        _http = new(handler ?? new HttpClientHandler { UseCookies = false, AllowAutoRedirect = false });
        _http.DefaultRequestHeaders.UserAgent.ParseAdd("Shovel-Avatars/1.0");
    }

    public async Task<BitmapSource?> GetAsync(AvatarSettings settings, string remote, string hash, string email, CancellationToken token)
    {
        var lookup = Lookup(settings, remote, hash, email);
        if (lookup is null) return null;
        var key = settings.Provider + "|" + settings.GiteaServer + "|" + remote + "|" +
            (string.IsNullOrWhiteSpace(email) ? hash : email.Trim().ToLowerInvariant());
        Task<BitmapSource?> work;
        lock (_cache)
        {
            if (_disposed) return null;
            if (_cache.TryGetValue(key, out var cached) && DateTimeOffset.UtcNow - cached.At < TimeSpan.FromHours(1)) work = cached.Task;
            else
            {
                // Scrolling must not create an unbounded download queue.
                if (_cache.Values.Count(value => !value.Task.IsCompleted) >= 32) return null;
                if (_cache.Count >= 128)
                    foreach (var old in _cache.Where(pair => pair.Value.Task.IsCompleted).OrderBy(pair => pair.Value.At).Take(32).Select(pair => pair.Key).ToArray()) _cache.Remove(old);
                var stopping = _stop.Token;
                var search = lookup.Value.Json && lookup.Value.Url.Host != "api.github.com" && !string.IsNullOrWhiteSpace(email)
                    ? new Uri(new Uri(settings.GiteaServer.TrimEnd('/') + "/"), "api/v1/users/search?q=" + Uri.EscapeDataString(email.Trim()) + "&limit=10") : null;
                work = Task.Run(() => LoadAsync(lookup.Value.Url, lookup.Value.Json, search, email, stopping));
                _cache[key] = (work, DateTimeOffset.UtcNow);
            }
        }
        return await work.WaitAsync(token);
    }

    public static (Uri Url, bool Json)? Lookup(AvatarSettings settings, string remote, string hash, string email)
    {
        if (settings.Provider == AvatarProvider.Initials || settings.ValidationCode is not null) return null;
        if (settings.Provider == AvatarProvider.Gravatar)
        {
            if (string.IsNullOrWhiteSpace(email)) return null;
            var digest = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(email.Trim().ToLowerInvariant()))).ToLowerInvariant();
            return (new Uri("https://www.gravatar.com/avatar/" + digest + "?s=64&d=404"), false);
        }
        if (hash.Length is not (40 or 64) || !hash.All(char.IsAsciiHexDigit)) return null;
        // Accept both normal URLs and Git's user@host:path SSH notation; never forward userinfo.
        if (!remote.Contains("://", StringComparison.Ordinal) && remote.IndexOf(':') is var colon && colon > 0)
            remote = "ssh://" + remote[..colon] + "/" + remote[(colon + 1)..];
        if (!Uri.TryCreate(remote, UriKind.Absolute, out var repo) || repo.Scheme is not ("https" or "http" or "ssh" or "git")) return null;
        var path = repo.AbsolutePath.Trim('/');
        if (path.EndsWith(".git", StringComparison.OrdinalIgnoreCase)) path = path[..^4];
        if (repo.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase))
        {
            var parts = path.Split('/'); if (parts.Length != 2 || parts.Any(part => part.Length == 0)) return null;
            return (new Uri("https://api.github.com/repos/" + path + "/commits/" + hash + "?per_page=1"), true);
        }
        if (!AvatarSettings.ValidServer(settings.GiteaServer)) return null;
        var server = new Uri(settings.GiteaServer.TrimEnd('/') + "/");
        if (!repo.Host.Equals(server.Host, StringComparison.OrdinalIgnoreCase)) return null;
        var prefix = server.AbsolutePath.Trim('/');
        if (prefix.Length > 0)
        {
            if (path.StartsWith(prefix + "/", StringComparison.Ordinal)) path = path[(prefix.Length + 1)..];
            else if (repo.Scheme is not ("ssh" or "git")) return null;
        }
        if (path.Split('/').Length != 2) return null;
        return (new Uri(server, "api/v1/repos/" + path + "/git/commits/" + hash + "?stat=false&verification=false&files=false"), true);
    }

    private async Task<BitmapSource?> LoadAsync(Uri url, bool json, Uri? search, string email, CancellationToken stopping)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(stopping);
        timeout.CancelAfter(TimeSpan.FromSeconds(8)); var token = timeout.Token; var entered = false;
        try
        {
            await _slots.WaitAsync(token); entered = true;
            if (json)
            {
                Uri? avatar = null;
                try
                {
                    var bytes = await DownloadAsync(url, 1024 * 1024, token);
                    using var data = JsonDocument.Parse(bytes);
                    if (data.RootElement.TryGetProperty("author", out var author) && author.ValueKind == JsonValueKind.Object &&
                        author.TryGetProperty("avatar_url", out var value)) Uri.TryCreate(value.GetString(), UriKind.Absolute, out avatar);
                }
                catch (HttpRequestException error) when (search is not null && error.StatusCode is System.Net.HttpStatusCode.NotFound or System.Net.HttpStatusCode.Forbidden or System.Net.HttpStatusCode.Unauthorized) { }
                // A public Gitea profile may be visible even when this repository is private.
                if (avatar is null && search is not null)
                {
                    using var data = JsonDocument.Parse(await DownloadAsync(search, 256 * 1024, token));
                    if (data.RootElement.TryGetProperty("data", out var users) && users.ValueKind == JsonValueKind.Array)
                        foreach (var user in users.EnumerateArray())
                            if (user.TryGetProperty("email", out var address) && string.Equals(address.GetString(), email.Trim(), StringComparison.OrdinalIgnoreCase) &&
                                user.TryGetProperty("avatar_url", out var imageUrl))
                            { Uri.TryCreate(imageUrl.GetString(), UriKind.Absolute, out avatar); break; }
                }
                if (avatar is null) return null; url = avatar;
            }
            var image = await DownloadAsync(url, 2 * 1024 * 1024, token);
            using var stream = new MemoryStream(image);
            var decoder = BitmapDecoder.Create(stream, BitmapCreateOptions.PreservePixelFormat, BitmapCacheOption.None);
            if (decoder.Frames.Count == 0 || decoder.Frames[0].PixelWidth > 4096 || decoder.Frames[0].PixelHeight > 4096) return null;
            stream.Position = 0;
            var bitmap = new BitmapImage(); bitmap.BeginInit(); bitmap.CacheOption = BitmapCacheOption.OnLoad;
            bitmap.DecodePixelWidth = 64; bitmap.StreamSource = stream; bitmap.EndInit(); bitmap.Freeze(); return bitmap;
        }
        catch (Exception error) when (error is HttpRequestException or IOException or JsonException or OperationCanceledException or
            NotSupportedException or ArgumentException or InvalidOperationException or System.Runtime.InteropServices.COMException)
        { return null; }
        finally { if (entered) _slots.Release(); }
    }
    private async Task<byte[]> DownloadAsync(Uri url, int maximum, CancellationToken token)
    {
        for (var redirects = 0; redirects < 4; redirects++)
        {
            if (url.Scheme is not ("https" or "http") || url.UserInfo.Length != 0) throw new IOException();
            using var response = await _http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, token);
            if ((int)response.StatusCode is >= 300 and < 400 && response.Headers.Location is { } next)
            { url = new Uri(url, next); continue; }
            response.EnsureSuccessStatusCode();
            if (response.Content.Headers.ContentLength > maximum) throw new IOException();
            await using var source = await response.Content.ReadAsStreamAsync(token); using var output = new MemoryStream();
            var buffer = new byte[8192]; int count;
            while ((count = await source.ReadAsync(buffer, token)) > 0)
            { if (output.Length + count > maximum) throw new IOException(); output.Write(buffer, 0, count); }
            return output.ToArray();
        }
        throw new IOException();
    }
    public void Reset()
    {
        lock (_cache)
        {
            if (_disposed) return;
            _stop.Cancel(); _stop.Dispose(); _stop = new(); _cache.Clear();
        }
    }
    public void Dispose() { lock (_cache) { _disposed = true; _stop.Cancel(); _http.Dispose(); _cache.Clear(); } }
}
