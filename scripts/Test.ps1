param([string]$ToolchainDirectory, [switch]$Offline)
. (Join-Path $PSScriptRoot 'Common.ps1')
Initialize-Toolchain $ToolchainDirectory
Push-Location $script:ProjectRoot
try {
    & (Join-Path $PSScriptRoot 'Format.ps1') -Check
    & (Join-Path $PSScriptRoot 'Verify-Vendor.ps1')
    $locked = @('--locked'); if ($Offline) { $locked += '--offline' }
    Invoke-Cargo (@('clippy', '--workspace', '--all-targets') + $locked + @('--', '-D', 'warnings'))
    Invoke-Cargo (@('test', '--workspace', '--all-targets') + $locked)
} finally { Pop-Location }
