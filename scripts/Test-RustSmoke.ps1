#requires -Version 7.4
# Explicit, bounded preview checks. No real repositories, desktop windows, network or installation.
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'Use-RustToolchain.ps1')
. (Join-Path $PSScriptRoot 'Get-RustSourceFingerprint.ps1')
$fingerprint=Get-RustSourceFingerprint $root
Push-Location $root
try{
    & cargo test -j 1 -p shovel-desktop -p shovel-core --lib --bins --test network --locked --no-run
    if($LASTEXITCODE -ne 0){throw 'Smoke test compilation failed.'}
    $checks=@(
        @('shovel-core','--lib','updates::tests::rust_urls_and_identifiers_are_separate'),
        @('shovel-core','--lib','updates::tests::verifies_powershell_pss_signature_and_exact_package'),
        @('shovel-core','--lib','updates::tests::rejects_modified_feed_wrong_key_and_swapped_channel'),
        @('shovel-core','--lib','updates::tests::rejects_validly_signed_foreign_product_and_unsafe_package'),
        @('shovel-core','--lib','settings::tests::imports_all_fields_without_modifying_csharp_and_roundtrips'),
        @('shovel-core','--lib','settings::tests::preserves_bad_or_newer_profiles_and_refuses_saves'),
        @('shovel-core','--lib','settings::tests::competing_writer_preserved_and_successful_save_keeps_backup'),
        @('shovel-core','--lib','settings::tests::explicit_fetch_interval_survives_and_channels_cannot_be_csharp'),
        @('shovel-core','--test=network','automatic_fetch_updates_refs_without_touching_worktree_and_honors_cancellation'),
        @('shovel-desktop','--bin=ShovelRust','workspace_interaction_tests::tab_drag_preserves_selection_restores_on_escape_and_saves_on_drop'),
        @('shovel-desktop','--bin=ShovelRust','workspace_interaction_tests::details_splitter_reserves_history_and_saves_drag_proportion'),
        @('shovel-desktop','--bin=ShovelRust','workspace_interaction_tests::reference_navigation_finds_old_tags_and_stashes_beyond_first_page'),
        @('shovel-desktop','--bin=ShovelRust','operations::tests::network_options_are_modal_persist_on_close_and_do_not_execute'),
        @('shovel-desktop','--bin=ShovelRust','operations::activity_layout_test::retained_operation_output_does_not_push_down_repository_tabs'),
        @('shovel-desktop','--bin=ShovelRust','git_identity::tests::global_identity_preserves_other_config_and_rejects_stale_review'),
        @('shovel-desktop','--bin=ShovelRust','updater::tests::obsolete_update_events_cannot_restore_previous_channel_or_progress'),
        @('shovel-desktop','--bin=ShovelRust','caption::tests::hidden_window_extends_client_area_and_separates_drag_from_buttons')
    )
    if($checks.Count -gt 30){throw 'Smoke count exceeded.'}
    $suite=[Diagnostics.Stopwatch]::StartNew()
    $results=foreach($check in $checks){
        $remaining=180000-[int]$suite.ElapsedMilliseconds
        if($remaining -le 0){throw 'Smoke suite exceeded three minutes.'}
        $info=[Diagnostics.ProcessStartInfo]::new((Get-Command cargo).Source)
        $info.WorkingDirectory=$root;$info.UseShellExecute=$false;$info.CreateNoWindow=$true
        $info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
        foreach($argument in @('test','-j','1','-p',$check[0],$check[1],$check[2],'--locked','--','--exact','--test-threads=1')){$info.ArgumentList.Add($argument)}
        $watch=[Diagnostics.Stopwatch]::StartNew()
        $process=[Diagnostics.Process]::Start($info)
        $stdout=$process.StandardOutput.ReadToEndAsync();$stderr=$process.StandardError.ReadToEndAsync()
        try{
            if(!$process.WaitForExit([Math]::Min(30000,$remaining))){$process.Kill($true);throw "Smoke deadline: $($check[2])"}
            $log=$stdout.GetAwaiter().GetResult()+$stderr.GetAwaiter().GetResult()
            if($process.ExitCode -ne 0 -or $log -notmatch 'test result: ok\. 1 passed; 0 failed'){
                Write-Output $log;throw "Smoke check failed or was not found: $($check[2])"
            }
            Write-Host "PASS $($check[2])"
            [ordered]@{Name=$check[2];Seconds=$watch.Elapsed.TotalSeconds}
        }finally{$process.Dispose()}
    }
    if((Get-RustSourceFingerprint $root) -ne $fingerprint){throw 'Source changed during smoke checks.'}
    $directory=Join-Path $root 'Build';New-Item -ItemType Directory -Path $directory -Force | Out-Null
    [ordered]@{SourceFingerprint=$fingerprint;Passed=$true;Count=$checks.Count;Seconds=$suite.Elapsed.TotalSeconds;Checks=@($results | Where-Object {$_ -is [Collections.IDictionary]});DesktopVerified=$false;InstalledUpdatesVerified=$false} |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $directory 'rust-smoke.json') -Encoding utf8NoBOM
    Write-Output "PASS $($checks.Count) Rust smoke checks in $([Math]::Round($suite.Elapsed.TotalSeconds,2)) seconds."
}finally{Pop-Location}
