Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression

function Expand-VerifiedRelease {
    param([Parameter(Mandatory)][string]$ArchivePath, [Parameter(Mandatory)][string]$Destination)
    $archive = (Resolve-Path -LiteralPath $ArchivePath).Path
    if ((Get-Item -LiteralPath $archive).Length -gt 64MB) { throw 'Release archive exceeds size limit.' }
    $checksum = Get-Content -LiteralPath ($archive + '.sha256') -Raw
    $pattern = '^([0-9a-fA-F]{64})\s+\*?' + [regex]::Escape([IO.Path]::GetFileName($archive)) + '\s*$'
    if ($checksum -notmatch $pattern) { throw 'Missing or invalid archive checksum record.' }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $Matches[1]) { throw 'Archive checksum mismatch.' }
    $destination = [IO.Path]::GetFullPath($Destination)
    if (Test-Path -LiteralPath $destination) { throw 'Extraction destination must not exist.' }
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $total = 0L
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName
            $total += $entry.Length
            if ($entry.Length -gt 64MB -or $total -gt 128MB -or $zip.Entries.Count -gt 2000) { throw 'Release archive exceeds extraction limits.' }
            if ($name -match '[\\:]|(^|/)\.\.?(/|$)|^/' -or -not $names.Add($name)) { throw 'Unsafe or duplicate archive entry.' }
            if ($name -notmatch '^(starrupture-(wave-)?overlay\.exe|LICENSE|README\.md|THIRD-PARTY-NOTICES\.md|manifest\.json|dependency-inventory\.json|licenses/[A-Za-z0-9_./+ -]+)$') { throw 'Unexpected release archive entry.' }
            $path = [IO.Path]::GetFullPath((Join-Path $destination $name))
            if (-not $path.StartsWith($destination + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Archive path escapes destination.' }
        }
        New-Item -ItemType Directory -Path $destination | Out-Null
        foreach ($entry in $zip.Entries) {
            if ($entry.FullName.EndsWith('/')) { continue }
            $path = Join-Path $destination $entry.FullName
            New-Item -ItemType Directory -Force -Path (Split-Path $path -Parent) | Out-Null
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $path, $false)
        }
    } finally { $zip.Dispose() }
    $manifest = Test-ReleaseDirectory $destination
    return $manifest
}

function Test-ReleaseDirectory {
    param([Parameter(Mandatory)][string]$Directory)
    $manifest = Get-Content -LiteralPath (Join-Path $Directory 'manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.schemaVersion -ne 1 -or $manifest.target -ne 'x86_64-pc-windows-msvc' -or $manifest.version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$' -or $manifest.revision -notmatch '^[0-9a-f]{40}$') { throw 'Invalid release manifest.' }
    $actual = @(Get-ChildItem -LiteralPath $Directory -Recurse -File | Where-Object { [IO.Path]::GetRelativePath($Directory, $_.FullName) -ne 'manifest.json' })
    $expected = @($manifest.files.PSObject.Properties)
    if ($actual.Count -ne $expected.Count) { throw 'Release file count mismatch.' }
    $root = [IO.Path]::GetFullPath($Directory)
    foreach ($entry in $expected) {
        if ($entry.Name -match '[\\:]|(^|/)\.\.?(/|$)|^/') { throw 'Unsafe manifest path.' }
        $path = [IO.Path]::GetFullPath((Join-Path $root $entry.Name))
        if (-not $path.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Manifest path escapes release.' }
        $file = Get-Item -LiteralPath $path
        if ($file.Length -ne $entry.Value.bytes -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value.sha256) { throw 'Release file integrity check failed.' }
    }
    foreach ($required in @('LICENSE', 'README.md', 'THIRD-PARTY-NOTICES.md', 'dependency-inventory.json', 'licenses/Oxanium/OFL.txt')) {
        if ($required -notin $expected.Name) { throw "Missing required release file: $required" }
    }
    $null = Get-ReleaseExecutableName $manifest
    return $manifest
}

function Get-ReleaseExecutableName {
    param([Parameter(Mandatory)]$Manifest)
    $names = @(@('starrupture-overlay.exe', 'starrupture-wave-overlay.exe') | Where-Object { $_ -in $Manifest.files.PSObject.Properties.Name })
    if ($names.Count -ne 1) { throw 'Release must contain exactly one recognised overlay executable.' }
    if (($Manifest.PSObject.Properties.Name -contains 'executable') -and $Manifest.executable -cne $names[0]) { throw 'Release executable does not match its file inventory.' }
    return $names[0]
}
