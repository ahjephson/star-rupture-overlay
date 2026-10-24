# Project-local installation only. Never installs or changes the system toolchain.
. (Join-Path $PSScriptRoot 'Common.ps1')
$version = '0.20.2'
$expected = '975a22143262fd27476d19ee00c7af67978426e40e1dee94eed6bbade1cf87dc'
$directory = Join-Path $script:ProjectRoot '.tools'
$archive = Join-Path $directory "cargo-deny-$version-x86_64-pc-windows-msvc.tar.gz"
New-Item -ItemType Directory -Force -Path $directory | Out-Null
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri "https://github.com/EmbarkStudios/cargo-deny/releases/download/$version/cargo-deny-$version-x86_64-pc-windows-msvc.tar.gz" -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
    throw 'cargo-deny archive does not match the reviewed checksum.'
}
& tar -xzf $archive -C $directory
if ($LASTEXITCODE -ne 0) { throw 'Could not extract cargo-deny.' }
