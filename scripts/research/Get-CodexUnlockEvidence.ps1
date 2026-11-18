param([Parameter(Mandatory)][string]$PrivateRoot,[Parameter(Mandatory)][string]$Request)
$ErrorActionPreference='Stop'
$project=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$root=(Resolve-Path -LiteralPath $PrivateRoot).Path
$local=(Resolve-Path -LiteralPath (Join-Path $project 'local')).Path
if(-not $root.StartsWith($local+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Unlock evidence must remain in ignored local storage'}
& (Join-Path $PSScriptRoot 'Assert-CodexRequest.ps1') -PrivateRoot $root -Request $Request
$nativePath=Join-Path $root 'mode-defaults/native-evidence.json'
$n=Get-Content $nativePath -Raw|ConvertFrom-Json -AsHashtable -Depth 100
$source=Get-Content (Join-Path $root 'index/source.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100
if($n.identity.executable_sha256 -cne $source.gameHash -or -not $n.readOnly -or $n.gameCodeInvoked){throw 'Native/source identity mismatch'}
$core=$n.baseCoreBinding.value.package
if(-not $core -or -not ($n.baseCoreBinding.owner.flags -band 16)){throw 'Validated Base Core settings binding required'}
$offers=@($n.weaponOfferEvidence.weapons.objects.path|Sort-Object -Unique)
if(-not $offers.Count){throw 'Owned current-world weapon offer evidence required'}
$index=Get-Content (Join-Path $root 'index/asset-paths.json') -Raw|ConvertFrom-Json
$candidates=@($index|Where-Object {$_ -match '/Plugins/GameFeatures/[^/]+/Content/.*/(BP_|I_)[^/]*Item[^/]*\.uasset$' -and $_ -notmatch '/(UI|Animations|Mods)/'}|ForEach-Object {$_ -replace '^\.\./\.\./\.\./StarRupture/Plugins/GameFeatures/([^/]+)/Content/','/$1/' -replace '\.uasset$',''})
$ids=@(@($core)+@($offers|ForEach-Object {$_.Split('.')[0]})+$candidates|Sort-Object -Unique)
if($ids.Count -gt 128){throw 'Unlock selection exceeds bounded probe capacity'}
$out=Join-Path $root 'unlock-evidence';[IO.Directory]::CreateDirectory($out)|Out-Null
$selection=Join-Path $out 'selection.json';ConvertTo-Json -InputObject $ids|Set-Content $selection -Encoding utf8NoBOM
& (Join-Path $project 'target/debug/examples/codex_probe.exe') $Request $out $selection
if($LASTEXITCODE){throw 'Unlock asset extraction failed'}
$tableIds=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
function Find-TextTables($Value){
    if($Value -is [string]){if($Value.StartsWith('/Game/Chimera/Localization/StringTables/',[StringComparison]::Ordinal)){$tableIds.Add($Value.Split('.')[0])|Out-Null};return}
    if($Value -is [Collections.IDictionary]){foreach($v in $Value.Values){Find-TextTables $v};return}
    if($Value -is [Collections.IEnumerable]){foreach($v in $Value){Find-TextTables $v}}
}
Find-TextTables (Get-Content (Join-Path $out 'raw-assets.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100)
$expanded=@($ids+@($tableIds)|Sort-Object -Unique)
if($expanded.Count -gt 128){throw 'Unlock text dependency bound exceeded'}
if($expanded.Count -gt $ids.Count){
    $ids=$expanded;ConvertTo-Json -InputObject $ids|Set-Content $selection -Encoding utf8NoBOM
    & (Join-Path $project 'target/debug/examples/codex_probe.exe') $Request $out $selection
    if($LASTEXITCODE){throw 'Unlock text extraction failed'}
}
Copy-Item -LiteralPath $nativePath -Destination (Join-Path $out 'native-evidence.json') -Force
[ordered]@{gameSha256=$source.gameHash;selectedMode=$n.selectedMode.class.path;coreAsset=$core;offeredWeaponClasses=$offers;rawSha256=(Get-FileHash (Join-Path $out 'raw-assets.json')).Hash.ToLowerInvariant();nativeSha256=(Get-FileHash (Join-Path $out 'native-evidence.json')).Hash.ToLowerInvariant();scope='native settings binding and current-world registered purchase candidates; not ownership or a universal weapon whitelist'}|ConvertTo-Json -Depth 20|Set-Content (Join-Path $out 'manifest.json') -Encoding utf8NoBOM
[ordered]@{selected=$ids.Count;registeredCandidates=$offers.Count;output=$out}|ConvertTo-Json -Compress