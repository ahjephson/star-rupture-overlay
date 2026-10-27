[CmdletBinding(DefaultParameterSetName = 'Install')]
param(
    [Parameter(Mandatory, ParameterSetName = 'Install')][string]$ArchivePath,
    [Parameter(Mandatory, ParameterSetName = 'Rollback')][switch]$Rollback,
    [string]$Destination = (Join-Path $env:LOCALAPPDATA 'Programs/StarRuptureWaveOverlay'),
    [switch]$Launch
)
. (Join-Path $PSScriptRoot 'ReleaseCommon.ps1')
$destination = [IO.Path]::GetFullPath($Destination)
# Do not touch or stop an installed running application. Users exit through the tray.
$running = @(Get-Process -Name @('starrupture-overlay', 'starrupture-wave-overlay') -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and $_.Path.StartsWith($destination + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
})
if ($running.Count) { throw 'Exit the installed overlay through its tray menu before deploying or rolling back.' }
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$statePath = Join-Path $destination 'installation.json'
$state = if (Test-Path -LiteralPath $statePath) { Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json } else { $null }
function Resolve-InstalledVersion([string]$Relative) {
    if ($Relative -notmatch '^versions/[0-9A-Za-z.+-]+$') { throw 'Invalid installed version path.' }
    return Join-Path $destination $Relative
}
if ($Rollback) {
    if (-not $state -or -not $state.previous) { throw 'No previous installation is available.' }
    $selected = Resolve-InstalledVersion $state.previous
    $manifest = Test-ReleaseDirectory $selected
    $next = [ordered]@{ current = $state.previous; previous = $state.current }
} else {
    $staging = Join-Path $destination ('staging-' + [Guid]::NewGuid().ToString('N'))
    $manifest = Expand-VerifiedRelease -ArchivePath $ArchivePath -Destination $staging
    $hash = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $relative = "versions/$($manifest.version)-$($hash.Substring(0,12))"
    $selected = Resolve-InstalledVersion $relative
    if (Test-Path -LiteralPath $selected) { throw 'This release is already installed; existing files were preserved.' }
    New-Item -ItemType Directory -Force -Path (Split-Path $selected -Parent) | Out-Null
    Move-Item -LiteralPath $staging -Destination $selected
    $next = [ordered]@{ current = $relative; previous = if ($state) { $state.current } else { $null } }
}
$temporary = Join-Path $destination ('installation-' + [Guid]::NewGuid().ToString('N') + '.tmp')
$next | ConvertTo-Json | Set-Content -LiteralPath $temporary -Encoding utf8NoBOM
[IO.File]::Move($temporary, $statePath, $true)
$exe = Join-Path $selected (Get-ReleaseExecutableName $manifest)
if ($Launch) { Start-Process -FilePath $exe -WindowStyle Hidden }
Write-Output $exe
