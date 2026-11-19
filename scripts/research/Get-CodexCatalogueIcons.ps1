param([Parameter(Mandatory)][string]$PrivateRoot,[Parameter(Mandatory)][string]$Request)
$ErrorActionPreference='Stop'
$project=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$root=(Resolve-Path -LiteralPath $PrivateRoot).Path
$local=(Resolve-Path -LiteralPath (Join-Path $project 'local')).Path
if(-not $root.StartsWith($local+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Icons must remain in ignored local storage'}
& (Join-Path $PSScriptRoot 'Assert-CodexRequest.ps1') -PrivateRoot $root -Request $Request
$c=Get-Content (Join-Path $root 'catalogue.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100
$ids=@(@($c.items.image.asset)+@($c.recipes.image.asset)+@($c.buildings.image.asset)|Where-Object {$_ -and $_.StartsWith('/Game/Chimera/',[StringComparison]::Ordinal)}|Sort-Object -Unique)
$out=Join-Path $root 'catalogue-icons';[IO.Directory]::CreateDirectory($out)|Out-Null
$files=@{};$errors=@()
for($offset=0;$offset -lt $ids.Count;$offset+=24){
    $batch=@($ids[$offset..([Math]::Min($offset+23,$ids.Count-1))]);$name='batch-{0:D3}' -f $offset
    $dir=Join-Path $out $name;$selection=Join-Path $out "$name-selection.json"
    ConvertTo-Json -InputObject $batch|Set-Content $selection -Encoding utf8NoBOM
    $raw=Join-Path $dir 'raw-assets.json'
    $refresh=-not (Test-Path -LiteralPath $raw)
    if(-not $refresh){$old=@(Get-Content $raw -Raw|ConvertFrom-Json -AsHashtable -Depth 100);$refresh=(@($old|Where-Object {$_.error -or @($_.exports|Where-Object {$_.error -or $_.extraError}).Count}).Count -gt 0) -or (($old.asset -join "`n") -cne ($batch -join "`n"))}
    if($refresh){& (Join-Path $project 'target/debug/examples/codex_probe.exe') $Request $dir $selection;if($LASTEXITCODE){throw 'Icon extraction failed'}}
    foreach($row in (Get-Content (Join-Path $dir 'raw-assets.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100)){
        $hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($row.asset))).ToLowerInvariant().Substring(0,24)
        $path="catalogue-icons/$name/icons/$hash.bgra"
        if(-not $row.error -and @($row.exports|Where-Object {$_.error -or $_.extraError}).Count -eq 0 -and $row.exports.extra.format -and (Test-Path -LiteralPath (Join-Path $root $path))){$files[$row.asset]=$path}else{$errors += [ordered]@{id=$row.asset;error=$row.error;exports=$row.exports}}
    }
}
[ordered]@{icons=$files;errors=$errors;requested=$ids.Count;decoded=$files.Count}|ConvertTo-Json -Depth 25|Set-Content (Join-Path $out 'manifest.json') -Encoding utf8NoBOM
[ordered]@{requested=$ids.Count;decoded=$files.Count;errors=$errors.Count}|ConvertTo-Json -Compress