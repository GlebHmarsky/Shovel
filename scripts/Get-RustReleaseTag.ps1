# Return the commit behind a lightweight or annotated remote version tag.
function Get-RustReleaseTag {
    param([string]$Root, [string]$Remote, $Plan)
    $ref = "refs/tags/$($Plan.Tag)"
    $peeled = "$ref^{}"
    $rows = @(git -C $Root ls-remote $Remote $ref $peeled)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect the remote Rust version tag.' }
    if (!$rows.Count) { return $null }
    $targets = @{}
    foreach ($row in $rows) {
        $hash, $name = $row -split "`t", 2
        if ($name -cnotin @($ref,$peeled) -or $targets.ContainsKey($name)) { throw 'Unexpected remote version tag response.' }
        $targets[$name] = $hash
    }
    $commit = $targets[$peeled] ?? $targets[$ref]
    if ($commit -cne $Plan.SourceCommit) { throw 'Existing immutable Rust version tag points to a different source commit.' }
    return $commit
}
