# Read-only baseline extraction; no generated entry is evidence of Rust parity.
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$baseline=(git -C $root rev-parse '2175458^{commit}').Trim()
if ($LASTEXITCODE -ne 0) { throw 'C# parity baseline unavailable' }
$entries=@()
foreach ($line in (git -C $root ls-tree -r $baseline -- src)) {
    if ($line -notmatch '^\d+ blob ([a-f0-9]+)\t(.+)$') { throw 'Unexpected Git tree entry' }
    $blob=$Matches[1]; $path=$Matches[2]
    $name=[IO.Path]::GetFileNameWithoutExtension($path)
    $parity=switch -Regex ($name) {
        'Update|SignedRelease' { 'R05','R08'; break }
        'AskPass|Authentication' { 'R56'; break }
        'Process|Child' { 'R10'; break }
        'Identity|MutationGate' { 'R11'; break }
        'Draft' { 'R18'; break }
        'Avatar' { 'R22'; break }
        'Activity|Transcript|OperationAttention|OperationProblem' { 'R23'; break }
        'Lifetime|ActionQueue|RepositoryOperations' { 'R09','R24'; break }
        'Settings|Layout|Panes|UiScale' { 'R14','R16','R17','R20'; break }
        'Theme|Palette|AppStyles|Caption' { 'R13','R14'; break }
        'Strings|UiText' { 'R12'; break }
        'Clone' { 'R27'; break }
        'Init' { 'R26'; break }
        'Checkout' { 'R48'; break }
        'BranchManagement' { 'R49'; break }
        'Remote' { 'R50'; break }
        'DefaultPull|DefaultNetwork|Network|AutoFetch|GitAction' { 'R47','R51','R52','R53','R54','R55'; break }
        'Conflict|Unmerged' { 'R62','R63','R64'; break }
        'Sequence|Recovery' { 'R61'; break }
        'Stash' { 'R58','R59'; break }
        'Sidebar' { 'R33','R67'; break }
        'StartupCache|HistoryCache' { 'R19'; break }
        'HistoryAction' { 'R60'; break }
        'HistoricalFile|HistoryFile' { 'R65','R66'; break }
        'FileHistory|FileInspection' { 'R66'; break }
        'History|Graph|CommitRow|RefBadge|Divergence' { 'R28','R29','R30','R31','R32'; break }
        'Binary' { 'R36'; break }
        'Hunk' { 'R39'; break }
        'Lfs' { 'R44'; break }
        'FileMenu|ChangeFileMenu|FileAction' { 'R42','R43','R45','R46'; break }
        'CommitFiles|FileDiff|SideBySide' { 'R34','R35'; break }
        'WorkingCopy|IndexSnapshot' { 'R38','R41'; break }
        'Commit|Changes' { 'R37','R38','R40','R41'; break }
        'Tools|FileShell' { 'R57'; break }
        'Tab|MainWindow' { 'R13','R15'; break }
        'RepositoryReader|RepositoryIdentity' { 'R25'; break }
        'Diagnostic|Performance|GitCommandLog' { 'R12','R23','R72'; break }
        default { 'R03','R13','R24' }
    }
    $entries += [ordered]@{Path=$path;GitBlob=$blob;ParityIds=@($parity);ReviewState='Pending';RustImplementation=$null;Evidence=$null}
}
$inventory=[ordered]@{SchemaVersion=1;BaselineCommit=$baseline;BaselineVersion='0.47.0';FileCount=$entries.Count;Note='Initial source-to-parity routing only. Each file still requires semantic review; no parity is implied.';Files=$entries}
$inventory | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $root 'docs/rust-source-inventory.json') -Encoding utf8NoBOM
Write-Output "Indexed $($entries.Count) baseline source files at $baseline."
