param([Parameter(Mandatory)][string]$ArchivePath, [Parameter(Mandatory)][string]$WorkDirectory)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$work = [IO.Path]::GetFullPath($WorkDirectory)
if (-not $work.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Regression fixtures must stay in this repository.' }
$archive = (Resolve-Path -LiteralPath $ArchivePath).Path
function Deploy([string[]]$Arguments, [bool]$Succeeds = $true) {
    $start = [Diagnostics.ProcessStartInfo]::new((Get-Command pwsh).Source)
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    foreach ($argument in (@('-NoProfile', '-File', (Join-Path $root 'scripts/Deploy.ps1')) + $Arguments)) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new(); $process.StartInfo = $start
    try {
        $null = $process.Start(); $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(60000)) { $process.Kill($true); throw 'Disposable deployment exceeded deadline.' }
        if (($process.ExitCode -eq 0) -ne $Succeeds) { throw ('Unexpected deployment result: ' + $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()) }
    } finally { $process.Dispose() }
}
function Check([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function State([string]$Folder) { Get-Content -LiteralPath (Join-Path $Folder 'installation.json') -Raw | ConvertFrom-Json }
function StateJson([string]$Folder) { (State $Folder) | ConvertTo-Json -Compress }
$baseline = [Collections.Generic.Dictionary[string,byte[]]]::new([StringComparer]::Ordinal)
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
    foreach ($entry in $zip.Entries) {
        $input = $entry.Open(); $memory = [IO.MemoryStream]::new()
        try { $input.CopyTo($memory); $baseline.Add($entry.FullName, $memory.ToArray()) } finally { $input.Dispose(); $memory.Dispose() }
    }
} finally { $zip.Dispose() }
$utf8 = [Text.UTF8Encoding]::new($false)
function Manifest($files) { [Text.Encoding]::UTF8.GetString($files['manifest.json']) | ConvertFrom-Json -AsHashtable }
function WriteManifest($files, $manifest) { $files['manifest.json'] = $utf8.GetBytes(($manifest | ConvertTo-Json -Depth 20 -Compress)) }
function Fixture([string]$Name, [scriptblock]$Change) {
    $files = [Collections.Generic.Dictionary[string,byte[]]]::new($baseline, [StringComparer]::Ordinal)
    & $Change $files | Out-Null
    $path = Join-Path $work ($Name + '.zip')
    if (Test-Path -LiteralPath $path) { throw 'Fixture already exists.' }
    $zip = [IO.Compression.ZipFile]::Open($path, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($pair in $files.GetEnumerator()) {
            $entry = $zip.CreateEntry($pair.Key, [IO.Compression.CompressionLevel]::Fastest)
            $stream = $entry.Open()
            try { $stream.Write($pair.Value) } finally { $stream.Dispose() }
        }
    } finally { $zip.Dispose() }
    ((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() + '  ' + [IO.Path]::GetFileName($path)) | Set-Content -LiteralPath ($path + '.sha256') -Encoding ascii
    return $path
}
$newName = 'starrupture-overlay.exe'; $oldName = 'starrupture-wave-overlay.exe'
$fresh = Join-Path $work 'fresh-installation'
Deploy @('-ArchivePath', $archive, '-Destination', $fresh)
Check (Test-Path -LiteralPath (Join-Path $fresh ((State $fresh).current + '/' + $newName))) 'Fresh executable missing.'
$legacy = Fixture 'legacy' {
    param($files)
    $files[$oldName] = $files[$newName]; $null = $files.Remove($newName)
    $manifest = Manifest $files
    $manifest.files[$oldName] = $manifest.files[$newName]; $null = $manifest.files.Remove($newName); $null = $manifest.Remove('executable')
    WriteManifest $files $manifest
}
$installed = Join-Path $work 'installation'
Deploy @('-ArchivePath', $legacy, '-Destination', $installed)
$first = State $installed
Check (Test-Path -LiteralPath (Join-Path $installed ($first.current + '/' + $oldName))) 'Legacy executable missing.'
$next = Fixture 'second' {
    param($files)
    $files['README.md'] = $files['README.md'] + $utf8.GetBytes("`nSynthetic deployment regression fixture.`n")
    $manifest = Manifest $files
    $manifest.files['README.md'] = @{bytes=$files['README.md'].Length;sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($files['README.md'])).ToLowerInvariant()}
    WriteManifest $files $manifest
}
Deploy @('-ArchivePath', $next, '-Destination', $installed)
$state = State $installed
Check ($state.previous -eq $first.current -and $state.current -ne $state.previous) 'Upgrade did not preserve the previous selection.'
Check (Test-Path -LiteralPath (Join-Path $installed ($state.current + '/' + $newName))) 'Upgraded executable missing.'
Deploy @('-Rollback', '-Destination', $installed)
$restored = StateJson $installed
Check ((State $installed).current -eq $first.current) 'Rollback did not restore legacy selection.'
$cases = [ordered]@{
    'two-executables' = { param($files) $files[$oldName]=$files[$newName]; $manifest=Manifest $files; $manifest.files[$oldName]=$manifest.files[$newName]; WriteManifest $files $manifest }
    'wrong-executable' = { param($files) $manifest=Manifest $files; $manifest.executable=$oldName; WriteManifest $files $manifest }
    'traversal' = { param($files) $files['../escaped.txt']=$utf8.GetBytes('escape') }
    'tampered' = { param($files) $files[$newName]=$utf8.GetBytes('tampered') }
    'extra' = { param($files) $files['licenses/extra/manifest.json']=$utf8.GetBytes('{}') }
    'duplicate-case' = { param($files) $files['license']=$files['LICENSE'] }
    'rooted' = { param($files) $files['/escaped.txt']=$utf8.GetBytes('escape') }
}
foreach ($name in $cases.Keys) {
    $bad = Fixture $name $cases[$name]
    Deploy @('-ArchivePath', $bad, '-Destination', $installed) $false
    Check ((StateJson $installed) -ceq $restored) 'Rejected package changed the installation selection.'
}
$bad = Join-Path $work 'bad-checksum.zip'; Copy-Item -LiteralPath $archive -Destination $bad
('0' * 64 + '  bad-checksum.zip') | Set-Content -LiteralPath ($bad + '.sha256') -Encoding ascii
Deploy @('-ArchivePath', $bad, '-Destination', $installed) $false
Check ((StateJson $installed) -ceq $restored) 'Rejected checksum changed installation selection.'
Check (-not (Test-Path -LiteralPath (Join-Path $work 'escaped.txt'))) 'Archive escaped work folder.'
Check (-not (Test-Path -LiteralPath (Join-Path $installed 'escaped.txt'))) 'Archive escaped installation folder.'
@{status='passed';installUpgradeRollback=$true;legacyNameUpgradeRollback=$true;rejectedArchiveCases=@($cases.Keys)+@('bad-checksum');appLaunched=$false} | ConvertTo-Json -Compress
