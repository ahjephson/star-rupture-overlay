. (Join-Path $PSScriptRoot 'Common.ps1')
$exe = Join-Path $script:ProjectRoot '.tools/cargo-deny-0.20.2-x86_64-pc-windows-msvc/cargo-deny.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Run scripts/Install-QualityTools.ps1 first.' }
Push-Location $script:ProjectRoot
try {
    & $exe check
    if ($LASTEXITCODE -ne 0) { throw 'Dependency advisory, licence, ban or source check failed.' }
} finally { Pop-Location }
