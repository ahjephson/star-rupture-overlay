Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:ProjectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

function Initialize-Toolchain {
    param([string]$ToolchainDirectory)
    if ($ToolchainDirectory) {
        $directory = (Resolve-Path -LiteralPath $ToolchainDirectory).Path
        if (-not (Test-Path -LiteralPath (Join-Path $directory 'cargo.exe'))) {
            throw 'Toolchain directory must contain cargo.exe.'
        }
        $env:PATH = $directory + [IO.Path]::PathSeparator + $env:PATH
    }
    $version = & rustc --version
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^rustc 1\.99\.1 ') {
        throw 'Use the pinned Rust 1.99.1 toolchain, including clippy and rustfmt.'
    }
    $env:CARGO_INCREMENTAL = '0'
    # Encode arguments independently so paths containing spaces remain single arguments.
    $flags = @('-D', 'warnings', '-C', 'target-feature=+crt-static', "--remap-path-prefix=$script:ProjectRoot=.")
    if ($env:CARGO_HOME) { $flags += "--remap-path-prefix=$($env:CARGO_HOME)=cargo-home" }
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
}

function Invoke-Cargo {
    param([Parameter(Mandatory)][string[]]$Arguments)
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed: $($Arguments -join ' ')" }
}

function Get-ProjectOutputPath {
    param([Parameter(Mandatory)][string]$Path)
    $full = [IO.Path]::GetFullPath((Join-Path $script:ProjectRoot $Path))
    if (-not $full.StartsWith($script:ProjectRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Build/package output must remain inside this project.'
    }
    return $full
}

function Write-JsonFile {
    param([Parameter(Mandatory)]$Value, [Parameter(Mandatory)][string]$Path)
    $Value | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM
}

function Get-FileInventory {
    param([Parameter(Mandatory)][string]$Directory)
    $inventory = [ordered]@{}
    Get-ChildItem -LiteralPath $Directory -File -Recurse | Sort-Object FullName | ForEach-Object {
        $relative = [IO.Path]::GetRelativePath($Directory, $_.FullName).Replace('\', '/')
        if ($relative -ne 'manifest.json') {
            $inventory[$relative] = [ordered]@{
                sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                bytes = $_.Length
            }
        }
    }
    return $inventory
}
