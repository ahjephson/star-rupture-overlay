param([Parameter(Mandatory)][string]$PrivateRoot,[Parameter(Mandatory)][string]$Request)
$ErrorActionPreference='Stop'
$pin=Get-Content (Join-Path $PrivateRoot 'index/source.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100
$q=Get-Content -LiteralPath $Request -Raw|ConvertFrom-Json -AsHashtable -Depth 30
if($q.gameHash -cne $pin.gameHash){throw 'Extraction request executable identity differs from the research source'}
if(-not [string]::Equals([IO.Path]::GetFullPath($q.paks),[IO.Path]::GetFullPath($pin.paks),[StringComparison]::OrdinalIgnoreCase)){throw 'Extraction request points to a different archive directory'}
$mapping=Get-Item -LiteralPath $q.mappings
if($mapping.Length -gt 16*1024*1024 -or (Get-FileHash -LiteralPath $mapping.FullName -Algorithm SHA256).Hash.ToLowerInvariant() -cne $pin.mappingsSha256){throw 'Extraction request schema differs from the pinned native schema'}