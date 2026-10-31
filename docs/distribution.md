# Windows installation and GitHub publishing

## Stable and Beta delivery (confirmed 2026-11-01)

Every task still produces a build. **Routine delivery publishes Beta only**: `pwsh -NoProfile -File scripts/Build-Release.ps1` defaults to `win-beta`. Stable requires the owner's explicit approval after testing: use `-Channel win-stable` on the approved source/version with its matching verified preview. Do not silently promote newer source. 0.35.0 is the explicitly authorized common starting version on both channels; subsequent versions go only to Beta until approved.

Stable releases use immutable `vVERSION-win-stable` tags and GitHub latest. Beta releases use immutable `vVERSION-win-beta` tags, prerelease status and `make_latest=false`; their signed feed is copied to the fixed `beta` prerelease after the versioned package is published. Channel selection in **Help → Shovel updates** is persisted (default Stable), applies to launch/manual checks and clears any pending package. A newer Beta is never downgraded when switching to Stable. Both installers use the same app/profile identity; they are alternative channels, not side-by-side applications.

Each channel is independently signed/verified, with channel-specific package names and replay floors. Feed-copy replacement can briefly expose an unavailable/mismatched Beta feed; verification fails closed and leaves the app usable, and retry uses the complete feed. Routine builds never advance Stable. [GitHub release API semantics](https://docs.github.com/en/rest/releases/releases) checked 2026-11-01. Anonymous verification supports `scripts/Test-PublishedUpdate.ps1 -ExpectedVersion VERSION -Channel win-beta` or `win-stable`. Publisher signing remains separate and unset.

Confirmed 2026-10-31: **[GlebHmarsky/Shovel](https://github.com/GlebHmarsky/Shovel)** is the single public MIT source repository and release host. New installers embed this repository; all future publication targets it. The permanent signing key and application identity are unchanged. No GitHub credentials are compiled into the app.
[Architecture](architecture.md#windows-installer-and-updater-0300) owns implementation;
[requirements](requirements.md#updates-and-distribution) owns acceptance;
[TODO](../TODO.md) owns outstanding work.

## Default delivery on master (2026-10-31)

From **0.34.0**, installed launches immediately check, download, verify and automatically restart into a newer release when the existing idle/save gate permits it. Failed or blocked attempts leave Shovel usable and retain manual retry. Periodic-check preferences do not disable launch checks. Older clients need one manual **Help → Shovel updates → Check and download → Restart and update** (or the latest installer) to acquire this behavior. Portable previews do not update themselves.

Every routine delivered build is a new immutable Beta prerelease; Stable needs explicit owner approval. Increment `Directory.Build.props` above
all previously published versions, write its changelog entry, then run:

```powershell
pwsh -NoProfile -File scripts/Build-Release.ps1
```

This verifies/builds the preview, packages the self-contained installer, signs the update feed,
publishes through the existing Git Credential Manager login and checks an anonymous full-package
download through the production updater. It does not install or restart the running application.
Commit before running the command. It requires clean master, pushes source, and verifies that the public master commit matches the installer metadata before creating its release tag. This publication is standing user authorization;
normal release work does not require another confirmation. Raw builds/focused tests remain local.
Explicit local-only output can use Build-Preview or Build-LocalTest without publishing.

Stable publishing uses the GitHub REST API, checks signature/hash/size and asset coverage, uploads
to a draft, verifies upload size/digest, then advances latest. GitHub CLI is not required. Existing
published versions and non-increasing versions are refused. A failed upload leaves an unpublished
draft; inspect it before retrying. Earlier releases are never overwritten. Source visibility and
keys are not changed by these commands. The legacy win-test publisher still requires GitHub CLI.

## Recover a blocked restart on 0.30.103 / 0.30.104

These installers predate schema-6 settings used by the development branch. With such a profile,
SettingsStore returns NewerSchema/CanSave=false, and the restart gate deliberately refuses to save
or overwrite the file. The generic blocked message in those versions did not identify this cause.

Close Shovel and install **0.30.105** once, preserving the existing profile. It supports the actual
schema-6 layout structure and retains commit/history layout fields during window capture. Then use
**Help → Shovel updates → Check and download → Restart and update** for **0.30.106**.
Do not delete the profile or change its SchemaVersion number. Future unknown formats still block
writes, now with explicit settings/installer guidance. The 0.30.106 release has the same fix and serves
as the online update target. Installed UI/restart confirmation remains a user acceptance check.

## Online rehearsal: 0.30.103 → 0.30.104

Both versions are published in shovel-releases. Install the regular **0.30.103** setup once, then
open **Help → Shovel updates → Check and download → Restart and update** to receive 0.30.104.
The installed **Shovel Local Test 0.30.102** uses a local file feed and a different app identity/key;
it cannot download these production packages. The regular installer lives at `%LOCALAPPDATA%\Shovel`
and uses the existing `%LOCALAPPDATA%\GitGui` settings/drafts. Local Test can be uninstalled separately.

Both builds passed 26 smoke checks (45.1 s and 45.2 s). The production `SignedReleaseSource` loaded
from the verified preview successfully fetched the public latest feed without credentials, verified
its permanent-key signature, downloaded the complete 68,696,154-byte 0.30.104 package, and verified
its SHA-256. It reread the feed and verified the package again before reporting success.
Reproduce with `scripts/Test-PublishedUpdate.ps1 -ExpectedVersion 0.30.104` while that version remains
in the latest feed. This does not install the update or automate visible UI; manual installed
restart acceptance remains pending. Windows publisher signing is still not configured.

## One repository and migration

Use Shovel for code, issues and Releases. Releases are tagged at the exact source commit used to build the installer. Public visibility allows downloads; only invited collaborators with write access can push changes.

0.32.1 is the migration build. Its identical signed artifacts are published once on the previous shovel-releases host as well, because already installed binaries still check that URL. Once installed, 0.32.1 checks only Shovel. The former repository is archived after both download paths are verified; it is not a future publishing destination. Deleting it before older installations migrate would break their update checks. A direct latest installer from Shovel is the recovery route if an old endpoint is eventually removed. Offline Local Test builds still require a regular installer.

No visibility change or old-host publication is performed by routine build scripts. All new release configurations and publishers accept only GlebHmarsky/Shovel. There is no custom update server.

## First release on the owner's PC

Requires Windows x64, the .NET SDK pinned by global.json, PowerShell 7.4+, Git for Windows,
and an existing GitHub login in Git Credential Manager for stable publishing. GitHub CLI is needed only for the optional win-test channel.
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
pwsh -File scripts/Publish-Release.ps1 -ArtifactDirectory 'Build/0.32.0-installer-win-stable'
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

The isolated offline `ShovelLocalTest` fixture now exercises native installation, signed local-feed
update from 0.30.101 to 0.30.102, profile-sentinel retention, native uninstall and scoped clean sweep
on this Windows PC. See [local test instructions](local-installer-test.md) and the kit's JSON results.
This uses a copied test identity and a CLI update probe; visible Help-menu/restart and production
GitHub transport remain unverified. Nothing was published. The remaining acceptance work below
still applies to production distribution.

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
