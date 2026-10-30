# Shovel Local Test — 0.30.101 → 0.30.102

This offline Windows x64 kit tests installation and updating without publishing a GitHub release.
It has a separate application identity, profile, drafts and signing key. It is not a production release.

## Try it

1. Open `Build/Shovel-Local-Test-0.30.101-to-0.30.102` in this worktree.
2. Run **Shovel-Local-Test-0.30.101-Setup.exe** as your normal Windows user.
3. Launch **Shovel Local Test** from Start. No administrator account is required.
4. Open **Help → Shovel updates**, choose **Check and download**, then **Restart and update**.
   The startup check may have already downloaded 0.30.102. Check that the installed version becomes 0.30.102.
5. Keep this kit and its `feed` subfolder at their original absolute location during the test.
   The offline app embeds that location; moving it breaks update checks.

The second installer, **Shovel-Local-Test-0.30.102-Setup.exe**, is available for repair/direct installation.
Install 0.30.101 first to test updating. Both are unsigned Windows executables; Windows may show an
unknown-publisher/SmartScreen warning. The update feed itself is RSA-signed, and packages are SHA-256 checked.
This signature is separate from Windows publisher signing.

## Where files live

| Item | Exact location for the current user |
| --- | --- |
| Application, updater and cached packages | `%LOCALAPPDATA%\ShovelLocalTest` |
| Executable | `%LOCALAPPDATA%\ShovelLocalTest\current\Shovel.exe` |
| Test settings | `%LOCALAPPDATA%\ShovelLocalTestData\Profile` |
| Test drafts | `%LOCALAPPDATA%\ShovelLocalTestData\Drafts` |
| Start shortcut | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Shovel Local Test.lnk` |
| Uninstall entry | `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\ShovelLocalTest` |

On this PC, `%LOCALAPPDATA%` is `C:\Users\hmars\AppData\Local`.
The normal Shovel profile (`%LOCALAPPDATA%\GitGui`) is separate. The test creates no startup task,
service, firewall rule or machine-wide installation. Normal Git actions inside the test app still
operate on whichever repository you deliberately open; use an empty/test repository for this check.

## Uninstall and clean sweep

Close Shovel Local Test and wait for updates to finish. In **Windows Settings → Apps → Installed apps**,
uninstall **Shovel Local Test**. Normal uninstall preserves test settings and drafts.

For full removal of this test installation and its test settings/drafts, run in PowerShell 7:

```powershell
pwsh -NoProfile -File .\Remove-LocalTest.ps1 -RemoveTestData
```

Run from the kit folder. Add `-WhatIf` to preview. Omit `-RemoveTestData` to keep settings and drafts.
The script refuses running test processes, unexpected identities and junctions/symlinks. It runs
the native uninstaller and checks its registration before deleting only the two fixed test folders.
It does not remove repositories, Git, credentials, production Shovel, or unrelated software.

You may then delete the kit folder to remove the installers and offline feed. Windows/Velopack may
retain diagnostic traces in `%LOCALAPPDATA%\velopack` and temporary files under `%TEMP%\velopack`;
these are shared framework locations and are deliberately not broadly deleted. Cleanup does not
promise forensic removal of Windows event logs or security scan history.

## Verification and limitations

`LOCAL-TEST-RESULT.json` records native installation of 0.30.101, the real signed updater service's
local download/verification/apply to 0.30.102, updated bootstrap, profile sentinel preservation,
native uninstall, and cleanup. `CLEANUP-TEST-RESULT.json` records a separate install of 0.30.102
followed by the standalone cleanup script, including shortcut and registry removal.

The automated update uses a test-only command-line probe and suppresses its restart; the normal
test UI still requests restart. Visible Help-menu interaction, repository gate behavior during an
installed update, preservation of real tabs/drafts, and the visible restart remain manual checks.
These results do not prove GitHub transport, Windows 10 compatibility, interrupted-update recovery
or SmartScreen reputation. No software test can guarantee zero risk.

The fixture is built by `scripts/Build-LocalTest.ps1` from a copied source tree. Only that copy has
the isolated identity, profile paths, offline transport, public test key, and lifecycle probes.
Production C# and production signing keys are unchanged. Never upload these fixture installers
as production releases. The local build refuses an existing output kit instead of overwriting it.

During automation, passing extra application arguments to the pinned Velopack setup caused a
native argument-parser panic before installation. The working test uses plain `--silent` setup;
ordinary double-click installation does not pass those extra arguments.
