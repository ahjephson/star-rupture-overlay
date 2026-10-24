param([Parameter(Mandatory)][string]$BuildDirectory, [string]$OutputDirectory = 'local/packages', [switch]$AllowUncommitted)
. (Join-Path $PSScriptRoot 'Common.ps1')
. (Join-Path $PSScriptRoot 'ReleaseCommon.ps1')
$build = Get-ProjectOutputPath $BuildDirectory
$output = Get-ProjectOutputPath $OutputDirectory
$manifest = Test-ReleaseDirectory $build
if ($manifest.uncommittedChanges -and -not $AllowUncommitted) { throw 'Official packages require a clean source checkout.' }
$suffix = if ($manifest.uncommittedChanges) { '-uncommitted' } else { '' }
$name = "starrupture-wave-overlay-v$($manifest.version)$suffix-windows-x64.zip"
New-Item -ItemType Directory -Force -Path $output | Out-Null
$path = Join-Path $output $name
if (Test-Path -LiteralPath $path) { throw 'Package already exists; never overwrite release assets.' }
$temporary = $path + "." + [Guid]::NewGuid().ToString("N") + ".tmp"
$zip = [IO.Compression.ZipFile]::Open($temporary, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($file in (Get-ChildItem -LiteralPath $build -File -Recurse | Sort-Object FullName)) {
        $relative = [IO.Path]::GetRelativePath($build, $file.FullName).Replace('\', '/')
        $entry = $zip.CreateEntry($relative, [IO.Compression.CompressionLevel]::Optimal)
        # Stable archive timestamps, independent of the builder's clock or filesystem.
        $entry.LastWriteTime = [DateTimeOffset]::new(2000, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
        $input = [IO.File]::OpenRead($file.FullName)
        $stream = $entry.Open()
        try { $input.CopyTo($stream) } finally { $stream.Dispose(); $input.Dispose() }
    }
} finally { $zip.Dispose() }
[IO.File]::Move($temporary, $path)
$hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $name" | Set-Content -LiteralPath ($path + '.sha256') -Encoding ascii
Write-Output $path
