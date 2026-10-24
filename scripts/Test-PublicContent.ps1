. (Join-Path $PSScriptRoot 'Common.ps1')
Push-Location $script:ProjectRoot
try {
    $files = & git ls-files
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inventory tracked files.' }
    $patterns = @('[A-Za-z]:[\/]+Users[\/]', 'gh[pousr]_[A-Za-z0-9]{20,}', 'github_pat_[A-Za-z0-9_]{20,}', '-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----', 'AKIA[0-9A-Z]{16}')
    foreach ($relative in $files) {
        if ($relative -match '(^|/)(local|dist|target|\.tools|\.codex)/|\.(sav|met|dmp|exe|dll|usmap)$') { throw "Private/generated file is tracked: $relative" }
        if ($relative.EndsWith('.ttf')) { continue }
        if (-not (Test-Path -LiteralPath $relative)) { continue } # staged deletion
        $text = Get-Content -LiteralPath $relative -Raw -Encoding utf8
        foreach ($pattern in $patterns) {
            if ($text -match $pattern) { throw "Sensitive-data pattern found in $relative; inspect locally." }
        }
    }
    $fontRoot = Join-Path $script:ProjectRoot 'crates/wave-overlay/assets/fonts'
    $fonts = Get-Content -LiteralPath (Join-Path $fontRoot 'manifest.json') -Raw | ConvertFrom-Json
    foreach ($font in $fonts.files) {
        if ((Get-FileHash -LiteralPath (Join-Path $fontRoot $font.file)).Hash.ToLowerInvariant() -ne $font.sha256) { throw 'Bundled font provenance changed.' }
    }
    Write-Output 'Tracked-content and embedded-font checks passed. This is a pattern check, not an exhaustive secret audit.'
} finally { Pop-Location }
