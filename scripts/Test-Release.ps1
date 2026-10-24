param([Parameter(Mandatory)][string]$ArchivePath)
. (Join-Path $PSScriptRoot 'Common.ps1')
. (Join-Path $PSScriptRoot 'ReleaseCommon.ps1')
$work = Join-Path $script:ProjectRoot ('local/release-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$unpacked = Join-Path $work 'unpacked'
$null = Expand-VerifiedRelease -ArchivePath $ArchivePath -Destination $unpacked
$exe = Join-Path $unpacked 'starrupture-wave-overlay.exe'
& python (Join-Path $PSScriptRoot 'inspect_pe.py') $exe
if ($LASTEXITCODE -ne 0) { throw 'Executable format or runtime dependency check failed.' }
foreach ($case in @('window', 'stacking', 'events', 'icons', 'render')) {
    $arguments = if ($case -eq 'window') { @('--self-test-window') } elseif ($case -eq 'stacking') { @('--self-test-stacking') } elseif ($case -eq 'events') { @('--self-test-window-events') } elseif ($case -eq 'icons') { @('--self-test-icons') } else { @('--self-test-dir', ('"' + (Join-Path $work 'render') + '"')) }
    $process = Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $work "$case.json") -RedirectStandardError (Join-Path $work "$case-stderr.txt")
    if (-not $process.WaitForExit(30000)) { $process.Kill(); throw 'Own-window diagnostic exceeded its deadline.' }
    if ($process.ExitCode -ne 0) { throw "Native $case check failed." }
}
& python (Join-Path $PSScriptRoot 'test_release.py') $ArchivePath $work
if ($LASTEXITCODE -ne 0) { throw 'Archive or deployment regression failed.' }
Write-Output "Release tests passed; private results: $work"
