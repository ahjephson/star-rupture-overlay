param([string]$ToolchainDirectory,[switch]$Offline)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
if($ToolchainDirectory){$resolved=(Resolve-Path -LiteralPath $ToolchainDirectory).Path;if(!(Test-Path -LiteralPath (Join-Path $resolved 'cargo.exe'))){throw 'Toolchain directory must contain cargo.exe'};$env:PATH=$resolved+';'+$env:PATH}
if(-not $env:CARGO_HOME){$env:CARGO_HOME=Join-Path $root '.tools/cargo'}
$env:CARGO_INCREMENTAL='0'
$env:RUSTFLAGS='-D warnings'
Push-Location $root
try {
 $extra=@('--locked');if($Offline){$extra+='--offline'}
 & cargo test --workspace --all-targets @extra;if($LASTEXITCODE -ne 0){throw 'Tests failed'}
 & cargo build --release --workspace --bins @extra;if($LASTEXITCODE -ne 0){throw 'Build failed'}
 New-Item -ItemType Directory -Force -Path (Join-Path $root 'dist'),(Join-Path $root 'local') | Out-Null
 $obsolete=Join-Path $root 'dist/starrupture-overlay-reader.exe'
 if(Test-Path -LiteralPath $obsolete){Remove-Item -LiteralPath $obsolete}
 $files=@{}
 foreach($name in @('starrupture-wave-overlay.exe')) {$src=Join-Path $root "target/release/$name";Copy-Item -LiteralPath $src -Destination (Join-Path $root "dist/$name");$files[$name]=@{sha256=(Get-FileHash -LiteralPath $src).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $src).Length}}
 Copy-Item -LiteralPath (Join-Path $root 'crates/wave-overlay/assets/fonts/OFL.txt') -Destination (Join-Path $root 'dist/Oxanium-OFL.txt')
 @{files=$files;sha256=$files['starrupture-wave-overlay.exe'].sha256;status='built'} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $root 'dist/manifest.json') -Encoding utf8NoBOM
 @{autoDiscover=$false;cacheDirectory='../local/cache'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'local/overlay.json') -Encoding utf8NoBOM
} finally {Pop-Location}

