param([Parameter(Mandatory)][string]$PrivateRoot,[Parameter(Mandatory)][string]$Request)
$ErrorActionPreference='Stop'
$project=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$root=(Resolve-Path -LiteralPath $PrivateRoot).Path
$local=(Resolve-Path -LiteralPath (Join-Path $project 'local')).Path
if(-not $root.StartsWith($local+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Evidence must remain in ignored local storage'}
& (Join-Path $PSScriptRoot 'Assert-CodexRequest.ps1') -PrivateRoot $root -Request $Request
$exe=Join-Path $project 'target/debug/examples/codex_probe.exe'
$full=Join-Path $root 'catalogue-evidence';[IO.Directory]::CreateDirectory($full)|Out-Null
$index=@(Get-Content (Join-Path $root 'index/asset-paths.json') -Raw|ConvertFrom-Json|Where-Object {$_ -like '../../../StarRupture/Content/Chimera/*.uasset'}|ForEach-Object {$_ -replace '^\.\./\.\./\.\./StarRupture/Content/','/Game/' -replace '\.uasset$',''})
$available=[Collections.Generic.HashSet[string]]::new([string[]]$index,[StringComparer]::Ordinal)
function Relevant([string]$Id){
    $leaf=$Id.Split('/')[-1]
    return $Id.StartsWith('/Game/Chimera/Localization/StringTables/',[StringComparison]::Ordinal) -or $Id.StartsWith('/Game/Chimera/Corporations/',[StringComparison]::Ordinal) -or $leaf -match '^(I_|BD_|DA_|CR_|CRC_|BR_)'
}
$records=[Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
$origins=@{}
foreach($dir in (Get-ChildItem -LiteralPath $full -Directory|Sort-Object Name)){
    $file=Join-Path $dir.FullName 'raw-assets.json'
    if(Test-Path -LiteralPath $file){foreach($row in (Get-Content $file -Raw|ConvertFrom-Json -AsHashtable -Depth 100)){$records[$row.asset]=$row;$origins[$row.asset]="$($dir.Name)/raw-assets.json"}}
}
$pending=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$missing=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
function Queue([string]$Id){
    $Id=$Id.Split('.')[0]
    if(-not (Relevant $Id)){return}
    if(-not $available.Contains($Id)){$missing.Add($Id)|Out-Null;return}
    if(-not $records.ContainsKey($Id)){$pending.Add($Id)|Out-Null}
}
function References($Value){
    if($Value -is [string]){if($Value.StartsWith('/Game/Chimera/',[StringComparison]::Ordinal)){Queue $Value};return}
    if($Value -is [Collections.IDictionary]){foreach($v in $Value.Values){References $v};return}
    if($Value -is [Collections.IEnumerable]){foreach($v in $Value){References $v}}
}
$native=Get-Content (Join-Path $root 'mode-defaults/native-evidence.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100
foreach($id in @('/Game/Chimera/Data/Flow/DA_Tutorial_Flow','/Game/Chimera/Data/Flow/DA_TutorialSkipped_Flow')){Queue $id}
foreach($entry in $native.selectedMode.authoredRoot.objects){Queue $entry.path}
foreach($id in $index){if($id -match '/Crafting/(CR_|CRC_)' -or $id -match '/I_[^/]+$' -or $id -match '/BuildingUpgrades/BR_' -or $id -match '/Corporations/(CD_|BuildingCollections/|DA_Custom)'){Queue $id}}
foreach($row in $records.Values){References $row}
for($round=0;$pending.Count -gt 0;$round++){
    if($round -ge 20 -or $records.Count+$pending.Count -gt 2500){throw 'Catalogue dependency closure exceeded research bounds'}
    $ids=@($pending|Sort-Object);$pending.Clear()
    for($offset=0;$offset -lt $ids.Count;$offset+=96){
        $last=[Math]::Min($offset+95,$ids.Count-1);$batch=@($ids[$offset..$last]);$name='batch-{0:D3}' -f $origins.Values.Count
        while(Test-Path -LiteralPath (Join-Path $full $name)){$name+='x'}
        $out=Join-Path $full $name;$selection=Join-Path $full "$name-selection.json"
        ConvertTo-Json -InputObject $batch|Set-Content -LiteralPath $selection -Encoding utf8NoBOM
        & $exe $Request $out $selection
        if($LASTEXITCODE){throw 'Bounded asset extraction failed'}
        foreach($row in (Get-Content (Join-Path $out 'raw-assets.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100)){$records[$row.asset]=$row;$origins[$row.asset]="$name/raw-assets.json"}
    }
    foreach($id in $ids){References $records[$id]}
}
[ordered]@{gameSha256=$native.identity.executable_sha256;packages=$records.Count;origins=$origins;missingReferencedPackages=@($missing|Sort-Object);scope='archive candidate inventory and bounded authored dependency closure; not eligibility by filename'}|ConvertTo-Json -Depth 12|Set-Content (Join-Path $full 'manifest.json') -Encoding utf8NoBOM
[ordered]@{packages=$records.Count;missing=$missing.Count;output=$full}|ConvertTo-Json -Compress