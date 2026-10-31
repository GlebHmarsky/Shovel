using System.IO;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using Velopack;
using Velopack.Logging;
using Velopack.Sources;

namespace GitGui.App.Services;

public sealed record UpdateConfiguration(string Repository, string PublicKey, string Channel)
{
    public bool IsConfigured => Regex.IsMatch(Repository, @"^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+$") &&
        PublicKey.Contains("-----BEGIN PUBLIC KEY-----", StringComparison.Ordinal) && Channel is "win-stable" or "win-test";
    public static UpdateConfiguration Load()
    {
        using var stream = typeof(UpdateConfiguration).Assembly.GetManifestResourceStream("GitGui.App.updates.json")!;
        return JsonSerializer.Deserialize<UpdateConfiguration>(stream) ?? new("", "", "win-stable");
    }
}

/// <summary>Original Velopack feed plus detached RSA signature. No GitHub API, token or device identifier.</summary>
public sealed class SignedReleaseSource(UpdateConfiguration configuration, HttpClient client, string? trustDirectory = null) : IUpdateSource
{
    public const long MaximumPackageSize = 512L * 1024 * 1024;
    public const int MaximumFeedSize = 1024 * 1024;
    private VelopackAsset[] _verified = [];
    public string FeedName => $"releases.{configuration.Channel}.json";
    public Uri FeedUri => new($"https://github.com/{configuration.Repository}/releases/" +
        (configuration.Channel == "win-stable" ? "latest/download/" : "download/test/") + FeedName);

    public async Task<VelopackAssetFeed> GetReleaseFeed(IVelopackLogger logger, string? appId, string channel,
        Guid? stagingId = null, VelopackAsset? latestLocalRelease = null)
    {
        if (!configuration.IsConfigured || channel != configuration.Channel || appId != "Shovel") throw new InvalidDataException();
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(30));
        var bytes = await ReadBoundedAsync(FeedUri, MaximumFeedSize, timeout.Token);
        var signature = await ReadBoundedAsync(new(FeedUri + ".sig"), 2048, timeout.Token);
        var feed = VerifyFeed(configuration, bytes, signature);
        if (trustDirectory is not null) await EnforceVersionFloorAsync(feed, trustDirectory, configuration.Channel, timeout.Token);
        _verified = feed.Assets;
        return feed;
    }

    public static VelopackAssetFeed VerifyFeed(UpdateConfiguration config, byte[] bytes, byte[] signature)
    {
        if (!config.IsConfigured || bytes.Length > MaximumFeedSize || signature.Length > 2048) throw new InvalidDataException();
        using var key = RSA.Create(); key.ImportFromPem(config.PublicKey);
        if (key.KeySize < 3072 || !key.VerifyData(bytes, signature, HashAlgorithmName.SHA256, RSASignaturePadding.Pss))
            throw new CryptographicException();
        var feed = VelopackAssetFeed.FromJson(new UTF8Encoding(false, true).GetString(bytes));
        if (feed.Assets.Length is 0 or > 100) throw new InvalidDataException();
        foreach (var asset in feed.Assets)
        {
            if (asset.PackageId != "Shovel" || asset.Type != VelopackAssetType.Full || asset.Version is null ||
                !Regex.IsMatch(asset.Version.ToString(), @"^\d+\.\d+\.\d+$") ||
                asset.FileName != $"Shovel-{asset.Version}-{config.Channel}-full.nupkg" ||
                asset.Size is <= 0 or > MaximumPackageSize || asset.SHA256 is not { Length: 64 } ||
                !asset.SHA256.All(char.IsAsciiHexDigit) || (asset.NotesMarkdown?.Length ?? 0) > 65536)
                throw new InvalidDataException();
        }
        return feed;
    }

    public static async Task EnforceVersionFloorAsync(VelopackAssetFeed feed, string directory, string channel, CancellationToken token)
    {
        if (channel is not ("win-stable" or "win-test")) throw new InvalidDataException();
        var path = Path.Combine(directory, "shovel-trusted-version-" + channel);
        var latest = feed.Assets.MaxBy(a => a.Version)!.Version;
        if (File.Exists(path))
        {
            if (new FileInfo(path).Length > 64) throw new InvalidDataException();
            var previous = SemanticVersion.Parse(await File.ReadAllTextAsync(path, token));
            if (latest < previous) throw new InvalidDataException();
            if (latest == previous) return;
        }
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            await File.WriteAllTextAsync(temporary, latest.ToString(), token);
            token.ThrowIfCancellationRequested(); File.Move(temporary, path, true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    public Uri PackageUri(VelopackAsset asset) => new($"https://github.com/{configuration.Repository}/releases/download/" +
        $"v{asset.Version}-{configuration.Channel}/{asset.FileName}");

    public async Task DownloadReleaseEntry(IVelopackLogger logger, VelopackAsset releaseEntry, string localFile,
        Action<int> progress, CancellationToken cancelToken = default)
    {
        if (!_verified.Contains(releaseEntry)) throw new InvalidDataException();
        try
        {
            using var response = await client.GetAsync(PackageUri(releaseEntry), HttpCompletionOption.ResponseHeadersRead, cancelToken);
            ValidateResponse(response);
            if (response.Content.Headers.ContentLength is { } length && length != releaseEntry.Size) throw new InvalidDataException();
            await using var input = await response.Content.ReadAsStreamAsync(cancelToken);
            await using (var output = new FileStream(localFile, FileMode.Create, FileAccess.Write, FileShare.None, 65536, true))
            {
                var buffer = new byte[65536]; long total = 0; int read, previousPercent = -1;
                while ((read = await input.ReadAsync(buffer, cancelToken)) != 0)
                {
                    total += read;
                    if (total > releaseEntry.Size) throw new InvalidDataException();
                    await output.WriteAsync(buffer.AsMemory(0, read), cancelToken);
                    var percent = (int)(total * 100 / releaseEntry.Size);
                    if (percent != previousPercent) { previousPercent = percent; progress(percent); }
                }
                if (total != releaseEntry.Size) throw new InvalidDataException();
            }
            await VerifyPackageAsync(releaseEntry, localFile, cancelToken);
        }
        catch
        {
            try { File.Delete(localFile); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            throw;
        }
    }

    public static async Task VerifyPackageAsync(VelopackAsset asset, string path, CancellationToken token)
    {
        await using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 65536, true);
        if (file.Length != asset.Size) throw new InvalidDataException();
        var actual = await SHA256.HashDataAsync(file, token);
        if (!CryptographicOperations.FixedTimeEquals(actual, Convert.FromHexString(asset.SHA256))) throw new CryptographicException();
    }

    private async Task<byte[]> ReadBoundedAsync(Uri uri, int maximum, CancellationToken token)
    {
        using var response = await client.GetAsync(uri, HttpCompletionOption.ResponseHeadersRead, token);
        ValidateResponse(response);
        if (response.Content.Headers.ContentLength > maximum) throw new InvalidDataException();
        await using var input = await response.Content.ReadAsStreamAsync(token);
        using var output = new MemoryStream(); var buffer = new byte[8192]; int read;
        while ((read = await input.ReadAsync(buffer, token)) != 0)
        { if (output.Length + read > maximum) throw new InvalidDataException(); output.Write(buffer, 0, read); }
        return output.ToArray();
    }
    private static void ValidateResponse(HttpResponseMessage response)
    {
        response.EnsureSuccessStatusCode();
        if (response.RequestMessage?.RequestUri?.Scheme != "https" ||
            response.Content.Headers.ContentType?.MediaType is "text/html" or "application/xhtml+xml") throw new InvalidDataException();
    }
}
