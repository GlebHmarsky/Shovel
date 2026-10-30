# Windows installation and GitHub publishing

Confirmed 2026-10-30: public source repository **Shovel**, MIT license, and separate public
**shovel-releases**, under the owner's personal GitHub account. Repositories are [GlebHmarsky/Shovel](https://github.com/GlebHmarsky/Shovel) and
[GlebHmarsky/shovel-releases](https://github.com/GlebHmarsky/shovel-releases). Both are private during preparation. No account credentials are compiled into the app.
[Architecture](architecture.md#windows-installer-and-updater-0300) owns implementation;
[requirements](requirements.md#updates-and-distribution) owns acceptance;
[TODO](../TODO.md) owns outstanding work.

## What to do on GitHub

The repositories already exist. Keep them private during preparation. When ready, make
**shovel-releases public** for unattended update access and make **Shovel public** for the
planned MIT source release. The app never carries a shared access token; while downloads
are private, an unauthenticated update check reports failure and leaves Shovel usable.
Source visibility does not affect updater access, which only uses shovel-releases.

Only invite collaborators who should have write access. Other people may read/fork public
source and propose pull requests, but cannot push directly without write access. No visibility
change is performed by the local packaging scripts. GitHub's generated source archives for
shovel-releases contain only that repository's files. No custom server is needed.

## First release on the owner's PC

Requires Windows x64, the .NET SDK pinned by global.json, PowerShell 7.4+, Git for Windows,
and (for scripted publishing only) GitHub CLI logged into the owner's account with `gh auth login`.
The project-local `vpk` tool and NuGet dependency are pinned to 1.2.161; no global tool installation.
Runtime packs are pinned to 10.0.12 for this delivery; refresh servicing deliberately before rollout.

The permanent signing key has been generated on this PC at
`%USERPROFILE%\.shovel-signing\shovel-update.key`, with current-user-only access.
Only `distribution/update-public.pem` is committed. Securely back up the private key outside
GitHub; all later releases must use the same key. On a replacement publishing PC, restore
this key rather than generating a new one. `New-UpdateKey.ps1` is available for a deliberately
new signing identity; do not use it for routine releases. Key rotation needs a bridge release
signed by the old key; loss/compromise needs a separate repair distribution.

Build once on final code, then package using that matching preview's smoke-verification record:

```powershell
pwsh -File scripts/Build-Preview.ps1
pwsh -File scripts/Build-Installer.ps1 `
  -PrivateKeyPath "$env:USERPROFILE/.shovel-signing/shovel-update.key"
```

The repository and public-key arguments default to the configured values. The scripts refuse mismatched
verification fingerprints and refuse overwriting an existing version. Output:
`Build/<version>-installer-win-stable`. The version comes from Directory.Build.props.
The installer includes .NET and AskPass; users still need Git for Windows. Install location is
`%LOCALAPPDATA%\Shovel`, with `current` application files and sibling updater/packages.
Existing settings/tabs and per-worktree drafts retain their GitGui locations outside that folder.

An unconfigured local installer can be built with `-LocalOnly` instead of repository/key arguments.
It has no update endpoint and cannot be published by the publishing script. It is for local
packaging validation, not the coworker rollout. Never treat that installer as an auto-updating release.

For Windows publisher signing, optionally pass `-AuthenticodeSignParameters` using your signing
provider's signtool arguments; Velopack signs the application, helper and installer. This is separate
from the mandatory signed update feed. No Windows certificate/service is configured by this delivery;
unsigned local installers can trigger SmartScreen/unknown publisher warnings. Signing does not
promise removal of every warning. Keep signing secrets out of command history and source control.

## Publish each release

```powershell
pwsh -File scripts/Publish-Release.ps1 -ArtifactDirectory 'Build/0.30.1-installer-win-stable'
```

This **publishes externally** when you run it. It checks the package's checksums, creates a draft
release `v<version>-win-stable`, uploads every asset, and only then makes it the latest stable release.
If upload fails, inspect/delete or finish that draft manually; no overwrite/replacement is attempted.
Do not use an older release's directory or edit generated feed/package bytes after signing.

Alternatively, use GitHub's Releases UI: create the matching versioned tag, upload **all files** in
the installer output directory to a draft, then publish it as the latest release. Retain earlier
versioned package assets: feeds refer to immutable versioned download URLs. Users initially download
`Shovel-Setup.exe`; installed apps use the signed Velopack feed and full package.

`-Channel win-test` builds the pilot channel. Scripted publishing first uploads its immutable
`v<version>-win-test` prerelease, then updates the fixed `test` prerelease's feed/signature.
Transient feed/signature mismatch is rejected and can be retried. Stable clients use latest stable;
test clients use the fixed test feed. Both use the same per-user installation identity, so pilot
machines install one channel at a time. There is no in-app channel switch or downgrade.

## User experience and recovery

Checks start 15–29 seconds after shell initialization, then optionally every 4 hours plus up to
29 minutes jitter while running. Help → Shovel updates exposes installed version, channel, host,
plain-text notes, progress, retry/cancel and the periodic setting. A small Update ready button
appears when a verified download is ready. Closing the dialog postpones installation.

Only **Restart and update** starts installation, after saving settings, checking all saved repositories
for active sequences/recovery and checking tool/action lifetime. An inaccessible/unvalidated tab
blocks installation; select/refresh or explicitly close that tab first. Shovel-launched editors and
terminals can hand off to an existing process, so the restart action asks whether that work is finished.
No force-close of a busy Shovel or automatic apply on ordinary launch/exit is requested.
Installed builds allow one running instance per installation, including custom profiles and Windows
sessions, to prevent another profile operating on replaced binaries. Portable developer profiles
retain their existing independent-profile behavior.

An exit/apply handoff marker blocks ordinary installed launches during replacement. Successful
framework restart/first-run clears it. If an update is interrupted, run the latest known-good installer
to repair it; do not promise automatic rollback. Uninstall removes application binaries; settings/drafts
outside the install folder are retained. Explicit app-data removal UI is not implemented. Repositories,
Git, shared credentials and external tools are not uninstall targets.

For a bad release, publish a higher-version correction signed by the same key. Withdrawing or changing
the feed prevents final validation from installing a withdrawn staged package; offline restart verification
fails safely. Keep a repair installer available independently if the app cannot start. Update failures
retain the installed app; retry uses full packages and may download again rather than resume a partial.

## Verification boundary

Headless checks and packaging do not prove installed replacement, restart, Windows 10 behavior,
SmartScreen, real GitHub connectivity, proxy/rate-limit behavior, native uninstallation or power-loss
recovery. Before coworker distribution, test two configured versions on a disposable Windows user/VM:
standard-user install, check/download, busy/recovery blocking, explicit restart, preserved tabs/drafts,
interrupted update/repair and uninstall. [Roadmap](roadmap.md#current-status) records actual evidence.

Vendor references checked 2026-10-30: [Velopack integration](https://docs.velopack.io/integrating/overview),
[installer](https://docs.velopack.io/packaging/installer),
[distribution](https://docs.velopack.io/distributing/overview),
[GitHub release access and limits](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases),
[personal repository permissions](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/repository-access-and-collaboration/permission-levels-for-a-personal-account-repository).
