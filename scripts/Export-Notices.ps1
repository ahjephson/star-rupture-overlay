param([Parameter(Mandatory)][string]$OutputDirectory, [switch]$Offline)
. (Join-Path $PSScriptRoot 'Common.ps1')
$arguments = @('metadata', '--locked', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc')
if ($Offline) { $arguments += '--offline' }
$metadata = & cargo @arguments | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata failed.' }
$packages = @{}; foreach ($package in $metadata.packages) { $packages[$package.id] = $package }
$nodes = @{}; foreach ($node in $metadata.resolve.nodes) { $nodes[$node.id] = $node }
$root = $metadata.packages | Where-Object name -eq 'starrupture-overlay'
$pending = [Collections.Generic.Stack[string]]::new(); $pending.Push($root.id)
$reachable = [Collections.Generic.HashSet[string]]::new()
while ($pending.Count) {
    $id = $pending.Pop()
    if (-not $reachable.Add($id)) { continue }
    foreach ($dependency in $nodes[$id].deps) { $pending.Push($dependency.pkg) }
}
$fallback = Get-Content -LiteralPath (Join-Path $script:ProjectRoot 'licenses/registry/index.json') -Raw | ConvertFrom-Json
$inventory = @()
foreach ($id in ($reachable | Sort-Object)) {
    $package = $packages[$id]
    if (-not $package.license) { throw "Missing licence declaration: $($package.name)" }
    $source = if ($package.source) { 'crates.io' } elseif ($package.manifest_path -like '*third-party*') { 'vendored' } else { 'project' }
    $inventory += [ordered]@{ name = $package.name; version = $package.version; license = $package.license; source = $source; repository = $package.repository }
    if ($source -eq 'project') { continue }
    $destination = Join-Path $OutputDirectory "licenses/$($package.name)-$($package.version)"
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    $directory = Split-Path $package.manifest_path -Parent
    $notices = @(Get-ChildItem -LiteralPath $directory -File | Where-Object Name -Match '^(?i:license|licence|copying|notice|copyright)')
    if (-not $notices.Count -and $source -eq 'vendored') {
        while (-not $notices.Count -and $directory -ne (Join-Path $script:ProjectRoot 'third-party')) {
            $directory = Split-Path $directory -Parent
            $notices = @(Get-ChildItem -LiteralPath $directory -File | Where-Object Name -Match '^(?i:license|licence|copying|notice|copyright)')
        }
    }
    if ($notices.Count) {
        foreach ($notice in $notices) { Copy-Item -LiteralPath $notice.FullName -Destination $destination }
    } else {
        $reviewed = @($fallback | Where-Object { $_.name -eq $package.name -and $_.version -eq $package.version })
        if ($reviewed.Count -ne 1) { throw "No reviewed redistribution notice for $($package.name) $($package.version)." }
        foreach ($file in $reviewed[0].files) {
            $path = Join-Path $script:ProjectRoot "licenses/registry/$($package.name)-$($package.version)/$($file.file)"
            if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) { throw 'Reviewed notice checksum mismatch.' }
            Copy-Item -LiteralPath $path -Destination $destination
        }
    }
}
$fonts = Join-Path $OutputDirectory 'licenses/Oxanium'
New-Item -ItemType Directory -Force -Path $fonts | Out-Null
Copy-Item -LiteralPath (Join-Path $script:ProjectRoot 'crates/wave-overlay/assets/fonts/OFL.txt') -Destination $fonts
Copy-Item -LiteralPath (Join-Path $script:ProjectRoot 'licenses/registry/index.json') -Destination (Join-Path $OutputDirectory 'licenses/registry-notice-sources.json')
Write-JsonFile ([ordered]@{ target = 'x86_64-pc-windows-msvc'; packages = $inventory }) (Join-Path $OutputDirectory 'dependency-inventory.json')

$setupSource = Join-Path $script:ProjectRoot 'licenses/asset-setup'
$setupDestination = Join-Path $OutputDirectory 'licenses/asset-setup'
$setupIndex = Get-Content -LiteralPath (Join-Path $setupSource 'index.json') -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Force -Path $setupDestination | Out-Null
foreach ($file in $setupIndex.notices) {
    $path = Join-Path $setupSource $file.file
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) { throw 'Recipe setup notice provenance changed.' }
    Copy-Item -LiteralPath $path -Destination $setupDestination
}
Copy-Item -LiteralPath (Join-Path $setupSource 'index.json') -Destination $setupDestination
