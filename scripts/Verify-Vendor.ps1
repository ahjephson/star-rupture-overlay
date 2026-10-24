$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$root = Join-Path $repo 'third-party'
$manifest = Get-Content -LiteralPath (Join-Path $root 'VENDOR.json') -Raw | ConvertFrom-Json
$changed = @()
foreach ($entry in $manifest.files) {
    $path = [IO.Path]::GetFullPath((Join-Path $repo $entry.path))
    if (-not $path.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid vendor manifest path' }
    if (-not (Test-Path -LiteralPath $path) -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { $changed += $entry.path }
}
$known = @($manifest.files.path) + 'third-party/VENDOR.json'
$extra = @(Get-ChildItem -LiteralPath $root -File -Recurse | Where-Object { [IO.Path]::GetRelativePath($repo, $_.FullName).Replace('\','/') -notin $known })
@{passed=($changed.Count -eq 0 -and $extra.Count -eq 0);checked=$manifest.files.Count;changed=$changed;unexpected=@($extra | ForEach-Object FullName)} | ConvertTo-Json
if ($changed.Count -or $extra.Count) { throw 'Vendored source differs from reviewed hashes' }
