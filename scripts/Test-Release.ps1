param([Parameter(Mandatory)][string]$ArchivePath)
. (Join-Path $PSScriptRoot 'Common.ps1')
. (Join-Path $PSScriptRoot 'ReleaseCommon.ps1')
$work = Join-Path $script:ProjectRoot ('local/release-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$unpacked = Join-Path $work 'unpacked'
$null = Expand-VerifiedRelease -ArchivePath $ArchivePath -Destination $unpacked
$manifest = Test-ReleaseDirectory $unpacked
$exe = Join-Path $unpacked (Get-ReleaseExecutableName $manifest)
$details = [Diagnostics.FileVersionInfo]::GetVersionInfo($exe)
$expectedDetails = @{
    FileDescription = 'StarRupture Overlay'
    ProductName = 'StarRupture Overlay'
    CompanyName = 'Alex Jephson'
    LegalCopyright = 'Copyright (c) 2026 Alex Jephson'
    OriginalFilename = 'starrupture-overlay.exe'
    InternalName = 'starrupture-overlay'
    FileVersion = $manifest.version
    ProductVersion = $manifest.version
}
foreach ($field in $expectedDetails.Keys) {
    if ($details.$field -cne $expectedDetails[$field]) { throw "Executable Details field $field is missing or incorrect." }
}
$versionParts = @($manifest.version.Split('-')[0].Split('.') | ForEach-Object { [int]$_ })
if ($details.FileMajorPart -ne $versionParts[0] -or $details.FileMinorPart -ne $versionParts[1] -or $details.FileBuildPart -ne $versionParts[2] -or $details.FilePrivatePart -ne 0 -or $details.ProductMajorPart -ne $versionParts[0] -or $details.ProductMinorPart -ne $versionParts[1] -or $details.ProductBuildPart -ne $versionParts[2] -or $details.ProductPrivatePart -ne 0) { throw 'Executable numeric version does not match the release.' }
Write-Output 'Executable Details metadata matches the release version.'
& python (Join-Path $PSScriptRoot 'inspect_pe.py') $exe
if ($LASTEXITCODE -ne 0) { throw 'Executable format or runtime dependency check failed.' }
foreach ($case in @('window', 'stacking', 'events', 'icons', 'render', 'dialogs', 'asset-cancel')) {
    $arguments = if ($case -eq 'window') { @('--self-test-window') } elseif ($case -eq 'stacking') { @('--self-test-stacking') } elseif ($case -eq 'events') { @('--self-test-window-events') } elseif ($case -eq 'icons') { @('--self-test-icons') } elseif ($case -eq 'dialogs') { @('--self-test-dialogs', ('"' + (Join-Path $work 'dialogs') + '"')) } elseif ($case -eq 'asset-cancel') { @('--self-test-asset-cancel', ('"' + (Join-Path $work 'asset-cancel') + '"')) } else { @('--self-test-dir', ('"' + (Join-Path $work 'render') + '"')) }
    $extension = if ($case -eq 'dialogs') { 'txt' } else { 'json' }
    $process = Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $work "$case.$extension") -RedirectStandardError (Join-Path $work "$case-stderr.txt")
    if (-not $process.WaitForExit(30000)) { $process.Kill(); throw 'Own-window diagnostic exceeded its deadline.' }
    if ($process.ExitCode -ne 0) { throw "Native $case check failed." }
}
& python (Join-Path $PSScriptRoot 'test_release.py') $ArchivePath $work
if ($LASTEXITCODE -ne 0) { throw 'Archive or deployment regression failed.' }
Write-Output "Release tests passed; private results: $work"
