# Requirements and acceptance

Updated 2026-10-24. **Confirmed** unless marked proposed. Feature delivery boundaries are owned by [roadmap](roadmap.md); [product](product.md) owns presentation and workflows. IDs below are stable references, not implementation classes.

## Git functionality and compatibility

- **G1:** Plan clone/init/open, status, file/hunk staging and unstaging, commits/amend, branches/tracking, fetch/pull/push, tags, stashes, merge, rebase, cherry-pick, revert, reset, reflog, and conflict resolution. Deferred commands must not appear functional.
- **G2:** Preserve installed Git for Windows behavior for configuration, credentials, hooks, signing, filters, and tools, subject to explicit trust/network boundaries. Detect missing Git and unsupported versions with remedies; no automatic installation or global configuration edits.
- **G3:** Model unborn/detached HEAD; dirty worktree; index versus worktree; untracked/ignored, rename/delete, binary and EOL changes. No assumption of 40-character hashes or loose ref storage.
- **G4:** Detect merge/rebase/cherry-pick/revert in progress; support valid continuation/skip/abort according to phase. Reopening must recover observed state, not execute an old intent.
- **G5:** Handle multiple remotes, missing/deleted upstreams, ahead/behind/divergence, rejected/non-fast-forward pushes, rewritten history, and branches checked out in other worktrees.
- **G6:** Detect submodules, linked worktrees, LFS, shallow/partial clones, sparse checkouts, and large repositories. Show unsupported operations explicitly; never silently expand sparse contents, fetch missing objects, bypass LFS, or recursively mutate submodules.
- **G7:** Preserve exact path identity across Unicode, spaces, leading dashes, unusual valid filenames, long paths, and case-only renames. Display unsupported Windows tree names without promising checkout. Handle permissions, locks, encoding, and external changes without data loss.

- **G8:** All Commits defaults to ancestry of local branches, remote-tracking branches and tags; distinguish current-HEAD history, including detached HEAD. Exclude reflog-only/unreachable history. Show ref labels, scope, active filters (or none), page range, loading and limits; older/newer navigation must not silently truncate. Selected details expose full message/metadata within explicit read limits.

## Execution, safety, and privacy

- **E1:** No Git, hooks, scripts, filesystem scanning, or expensive parsing on the UI thread. Use async processes, bounded concurrency/output, streaming, incremental loading, and virtualized views.
- **E2:** Coordinate mutations including shared worktree state; reject stale results and recheck prerequisites. External clients remain independent; app locks are not global Git transactions.
- **E3:** Show operation, progress, duration, output and running/input/conflict status. Handle SSH passphrases/host keys, credential-manager/browser auth, signing, hooks, editors, pagers, interactive rebase explicitly; no invisible input waits or inferred waiting state based only on silence.
- **E4:** Cancellation manages owned descendants, never shared agents/browsers. It is not rollback. Reinspect after cancellation/failure/crash/resume; handle network loss, disk full and stale locks without automatic unsafe cleanup or mutation retries.
- **S1:** Never silently discard, clean, reset, rewrite history, force-push, bypass hooks, or change global config. Preview destructive scope and confirm; prefer explicit force-with-lease. Explain recovery limits, including untracked/uncommitted content not protected by reflog.
- **S2:** Establish repository trust before repository-controlled hooks, filters, tools, helpers, or scripts execute. Ownership exceptions such as safe.directory are not trust. No automatic host-key acceptance or trust expansion.
- **S3:** Treat repository data, filenames, remote URLs, output, and release notes as untrusted. Use structured arguments/machine-readable output, literal path handling, and safe text rendering; never shell-interpolate untrusted values.
- **S4:** Use established credential storage; no plaintext secrets or embedded shared passwords/tokens. Keep Git wording alongside explanations, with sensitive values redacted before logging/export. Bound retained diagnostics; arbitrary hook output may contain secrets.
- **S5:** No telemetry, analytics, repository uploads, avatar requests, auto-fetch, or other unsolicited network features by default. **Explicit exception:** requested startup updater checks to the disclosed release endpoint; optional periodic checks are configurable. They transmit no repository content. This supersedes the earlier research suggestion that all update checks start disabled. Disclose necessary HTTP metadata/server access logs; do not promise network anonymity.

## Localization, accessibility, and lifecycle

- **L1:** Every app-owned menu, dialog, error, tooltip, accessibility label, installer/updater message supports localization from inception. Complete resource messages, plural rules, culture-aware dates/numbers, expansion, RTL, and direction isolation for code/paths/hashes; no sentence concatenation. Git output remains original text.
- **A1:** Keyboard-only navigation, visible/restored focus, accessible context menus/dialogs, screen-reader semantics including custom graph/diff views, text/icon status beyond color, dark/light themes, high contrast, DPI scaling.
- **B1:** Preserve tab order/selection and settings. Optional tray with explicit close/exit and no surprise autostart; minimal hidden/idle work. No dependency on hosted services for local Git use.

## Updates and distribution

- **U1:** Install once, in-place updates for approximately 30 Windows 10/11 users. Prefer mature stack-compatible updater, static HTTPS manifest, and per-user no-admin install where feasible. No custom backend/permanent service absent demonstrated need. Flow is specified in [product](product.md#update-experience-confirmed).
- **U2:** Manifest carries version, package location, verification metadata, release notes; support stable/test channels and a small pilot. Manual check, installed version, meaningful status, and postponement are required.
- **U3:** Never interrupt an active Git command, hook, auth/signing prompt, tracked external tool, or conflict-resolution workflow. Recheck all sessions/instances immediately before shutdown. Refuse unknown/busy state; prevent new work racing the check. No automatic apply on later launch/exit that bypasses explicit consent and this gate.
- **U4:** Verify package authenticity before executing/installing; distinguish updater signatures from Windows code signing. Protect keys, verify trusted metadata/package correspondence, reject invalid signatures and wrong channel/architecture/version. A checksum or HTTPS alone is not a publisher signature.
- **U5:** Offline hosting, timeout, proxy/auth failure, interrupted download, insufficient space, install failure, and crashes have explicit recoverable status. Check/download failures leave the installed version usable. Never mistake a login page for a package; never disable verification to recover.
- **U6:** Preserve settings, tabs/drafts and recovery data through versioned schema migration. Document bad-release recovery. No automatic rollback claim until supported by chosen updater and tested, including data-schema compatibility.
- **U7:** Private distribution uses approved access controls without shipped shared secrets. Outside-office access must be documented. Publish complete immutable packages before promoting the manifest; hosting failure must not disable installed apps.
- **D1:** Clean uninstall, dependency-license notices, signed distribution options. Never remove repositories, installed Git, shared credentials, or external tools. Data-retention choice for app settings/cache is explicit.

## Verification and acceptance

The cases below translate confirmed requirements into a **proposed verification specification**; numerical budgets and test parameters are not approved or measured results.

Run meaningful parser/unit and disposable-repository integration tests, followed by actual installed Windows runtime tests. Do not execute destructive tests on user repositories. Use isolated Git config and explicit test hooks; live auth uses approved test accounts. Record OS/build, architecture, Git/runtime versions, fixture and test evidence. Build success is not runtime proof.

| Gate | Pass condition |
| --- | --- |
| Foundation F0 | Restore/build cleanly; real resizable shell, clear unavailable Git controls, English resources, async profile I/O, safe diagnostic fields; corrupt/newer settings and failed/cancelled saves preserve existing bytes. Verify actual launch, layout changes, close and relaunch separately from headless checks. |
| Metadata F1a | Structured arguments round-trip; both output streams drain within caps; timeouts/cancellation stop owned descendants, including inherited-pipe cases. Trusted discovery rejects missing/older Git. Disposable unborn/detached/linked/conflicted worktrees produce correct metadata. Denied trust and restored tabs run no commands; shutdown cancels/drains reads; stale completions cannot populate another selection. Verify published helper files and launch separately. |
| History F1b | Real topologically ordered rows match CLI fixtures; left graph nodes connect actual parents and right columns show description/author/date/hash. Selection exposes corresponding message/metadata. Bounded expansion pins the tip and preserves selection; shallow/empty/failure states are explicit. Malformed output fails safely; signature helpers never run; stale history cannot enter a new tab. Offscreen WPF checks prove columns, graph cells and limited realized rows; interactive runtime remains a separate gate. |
| History F1c | Default All Commits matches CLI across local/remote/tag-only ancestors, merges, annotated/nested tags and packed refs. All 2,005 fixture commits remain accessible across bounded pages, without duplicates or broken graph boundaries. Detached/unborn/empty scopes are distinct; scope changes never checkout. Ref changes require refresh; failed/cancelled pages preserve visible rows. Labels, full selected details and paging controls bind headlessly; actual visual/keyboard verification remains separate. |
| Git reads | Status/index/worktree and commit/diff results match CLI fixtures, including empty, detached, binary, renamed, deleted, ignored, EOL and unusual paths. |
| Mutations | Expected refs/index/files change and unrelated data stays intact; stale hunk and external-edit races fail safely. |
| Recovery | Conflicts and interrupted sequences survive restart; only valid actions enabled; cancellation owns the correct children and refreshes state. |
| Failure | Failed hooks/signing, auth/network loss, disk full, permissions and existing locks yield usable UI and preserved diagnostics, without unsafe retry/cleanup. |
| Trust/privacy | Untrusted repo cannot execute configured test helper; background browsing causes no implicit network fetch; secret fixtures do not appear in retained logs. |
| Accessibility/localization | Windows 10/11 keyboard and screen-reader pass; 100/150/200% and mixed-monitor DPI; +40% pseudo-localization and RTL without inaccessible/clipped actions. |

### Proposed performance budgets (not measurements)

Release build, documented 4-core/8-GB/SSD reference machine; record antivirus/cache conditions. At least 20 runs for p95 claims; cold runs after reboot, warm runs after prior launch. Separate shell readiness from Git load; report child-process peaks as well as app memory.

| Metric | Proposed acceptance |
| --- | --- |
| Interactive shell | Warm p95 <=1 s; cold p95 <=2.5 s |
| Initial status + 200 commits | p95 <=2 s on 10,000-file/100,000-commit fixture |
| Idle private bytes | <=180 MiB for one repo; <=300 MiB with ten restored tabs/inactive views unloaded |
| Idle CPU | <=0.2% total CPU average over 5 min; zero periodic Git launches while unchanged |
| Input latency under load | p95 <=100 ms; no app-caused UI-thread stall >200 ms |
| Installer | Compressed self-contained x64 <=100 MiB, excluding separately installed Git |
| Scaling | Million-commit fixture remains interactive; bounded caches, no full-history UI allocation |

### Update acceptance matrix

Test real signed N -> N+1 installations on agreed Windows 10/11 builds, standard user, both channels. Use disposable user profiles/repositories and a controllable test endpoint; do not configure production hosting for tests.

| Test | Required result |
| --- | --- |
| Successful upgrade | Explicit restart installs verified N+1 in place, no uninstall step/elevation where supported; version/status correct. |
| Postpone | N remains usable; no install on idle/ordinary exit/next startup without renewed explicit action. |
| Hosting unavailable | Offline/DNS/timeout/5xx and proxy/401/403 failures do not block launch or Git; manual retry works; no tight retry loop. |
| Invalid content | Altered manifest/package, wrong signer, truncated response, wrong architecture/channel and login HTML rejected before execution. |
| Interrupted download | Cancel, connectivity loss and app crash leave N usable; retry safely restarts or resumes then re-verifies all content. |
| Preserved data | Settings, tab order/selection, draft and recovery markers preserved; migration backup exists; invalid/newer schema handled without overwriting it. |
| Busy installation | Each Git/hook/auth/external-tool/conflict state blocks install, including activity starting between click and shutdown and in another app instance. |
| Resource/install failure | Low disk, locked files, denied permissions and installer/app crash do not corrupt user data; follow documented repair path. No untested rollback assertion. |
| Bad release/channel | Pilot receives test release only; stable unaffected. Withdraw bad manifest and exercise higher-version corrective release plus documented offline repair. |
| Publication race | Package exists and verifies before manifest exposure; manifest/signature mismatch fails closed and later recovers; old installed version remains usable. |
