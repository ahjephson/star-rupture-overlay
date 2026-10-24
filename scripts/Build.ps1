param([string]$ToolchainDirectory, [switch]$Offline, [string]$OutputDirectory = 'dist')
. (Join-Path $PSScriptRoot 'Common.ps1')
Initialize-Toolchain $ToolchainDirectory
$output = Get-ProjectOutputPath $OutputDirectory
# Do not overwrite the running installation or mingle releases with old/generated data.
if ((Test-Path -LiteralPath $output) -and @(Get-ChildItem -LiteralPath $output -Force).Count) {
    throw 'Build output must be empty. Choose a new OutputDirectory; deploy separately.'
}
Push-Location $script:ProjectRoot
try {
    & (Join-Path $PSScriptRoot 'Test.ps1') -Offline:$Offline
    $locked = @('--locked'); if ($Offline) { $locked += '--offline' }
    Invoke-Cargo (@('build', '--release', '--target', 'x86_64-pc-windows-msvc', '-p', 'starrupture-wave-overlay') + $locked)
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $script:ProjectRoot 'target' }
    $exe = Join-Path $target 'x86_64-pc-windows-msvc/release/starrupture-wave-overlay.exe'
    New-Item -ItemType Directory -Force -Path $output | Out-Null
    Copy-Item -LiteralPath $exe -Destination $output
    foreach ($name in @('LICENSE', 'README.md', 'THIRD-PARTY-NOTICES.md')) {
        Copy-Item -LiteralPath (Join-Path $script:ProjectRoot $name) -Destination $output
    }
    & (Join-Path $PSScriptRoot 'Export-Notices.ps1') -OutputDirectory $output -Offline:$Offline
    $metadata = (& cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json)
    if ($LASTEXITCODE -ne 0) { throw 'Could not read version metadata.' }
    $version = ($metadata.packages | Where-Object name -eq 'starrupture-wave-overlay').version
    $revision = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'A Git checkout is required for release provenance.' }
    $dirty = [bool](& git status --porcelain)
    $manifest = [ordered]@{
        schemaVersion = 1; version = $version; revision = $revision; uncommittedChanges = $dirty
        target = 'x86_64-pc-windows-msvc'; toolchain = '1.99.1'; files = (Get-FileInventory $output)
    }
    Write-JsonFile $manifest (Join-Path $output 'manifest.json')
    Write-Output "Candidate built at $output. The running overlay was not replaced."
} finally { Pop-Location }
