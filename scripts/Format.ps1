param([switch]$Check)
. (Join-Path $PSScriptRoot 'Common.ps1')
Push-Location $script:ProjectRoot
try {
    # cargo fmt --all can traverse path dependencies; never reformat vendor source.
    $arguments = @('--edition', '2024', '--config', 'skip_children=true')
    if ($Check) { $arguments += '--check' }
    Get-ChildItem -LiteralPath 'crates' -Filter '*.rs' -Recurse -File | ForEach-Object {
        & rustfmt @arguments $_.FullName
        if ($LASTEXITCODE -ne 0) { throw "Rust formatting failed: $($_.Name)" }
    }
} finally { Pop-Location }
