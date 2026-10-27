param(
    [Parameter(Mandatory)][string]$CatalogueFile,
    [Parameter(Mandatory)][string]$TexturesDirectory,
    [Parameter(Mandatory)][string]$FontDataFile,
    [Parameter(Mandatory)][string]$OutputDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
# Local owner-supplied game exports. Never downloaded or embedded in source.
Add-Type -AssemblyName System.Drawing
$catalogue=Get-Content -LiteralPath $CatalogueFile -Raw | ConvertFrom-Json -Depth 100
if(Test-Path -LiteralPath $OutputDirectory){throw 'Choose a new asset output directory.'}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$images=@{}
function Export-Image([string]$ObjectPath){
    $asset=$ObjectPath -replace '\.\d+$',''
    if($images.ContainsKey($asset)){return $images[$asset]}
    if(-not $asset.StartsWith('/Game/')){throw "Unsupported image reference: $asset"}
    $relative='StarRupture/Content/'+$asset.Substring(6)+'.png'
    $source=Join-Path $TexturesDirectory $relative
    if(-not (Test-Path -LiteralPath $source)){throw "Missing exported image: $asset"}
    $sha=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($asset))).ToLowerInvariant()
    $filename=$sha.Substring(0,24)+'.bgra'
    $bitmap=[Drawing.Bitmap]::FromFile($source)
    try {
        $w=$bitmap.Width; $h=$bitmap.Height
        if($w -lt 1 -or $h -lt 1 -or $w -gt 1024 -or $h -gt 1024){throw 'Image dimensions exceed bound.'}
        $rect=[Drawing.Rectangle]::new(0,0,$w,$h)
        $data=$bitmap.LockBits($rect,[Drawing.Imaging.ImageLockMode]::ReadOnly,[Drawing.Imaging.PixelFormat]::Format32bppPArgb)
        try {
            $pixels=[byte[]]::new($w*$h*4)
            for($y=0;$y -lt $h;$y++){[Runtime.InteropServices.Marshal]::Copy([IntPtr]::Add($data.Scan0,$y*$data.Stride),$pixels,$y*$w*4,$w*4)}
            $stream=[IO.File]::Create((Join-Path $OutputDirectory $filename))
            try{$stream.Write([BitConverter]::GetBytes([uint32]$w));$stream.Write([BitConverter]::GetBytes([uint32]$h));$stream.Write($pixels)}finally{$stream.Dispose()}
        }finally{$bitmap.UnlockBits($data)}
    }finally{$bitmap.Dispose()}
    $images[$asset]=$filename
    return $filename
}
$recipes=[ordered]@{}; $items=[ordered]@{}; $wanted=[Collections.Generic.HashSet[string]]::new()
foreach($r in $catalogue.recipes | Where-Object unlockType -eq 'ECrRecipeUnlockType::RecipeTable'){
    if($null -eq $r.output -or $null -eq $r.researchRequirements){continue}
    $requirements=[ordered]@{}
    foreach($p in $r.researchRequirements.PSObject.Properties){$requirements[$p.Name]=[uint32]$p.Value;[void]$wanted.Add($p.Name)}
    [void]$wanted.Add($r.output.itemId)
    $recipes[$r.id]=[ordered]@{name=$r.name;itemId=$r.output.itemId;requirements=$requirements}
}
foreach($id in $wanted){
    $item=$catalogue.items | Where-Object id -eq $id | Select-Object -First 1
    if($null -eq $item){throw "Missing item definition: $id"}
    $name=$item.name
    if($item.defaultProperties.ItemName.LocalizedString){$name=$item.defaultProperties.ItemName.LocalizedString}
    $items[$id]=[ordered]@{name=$name;image=(Export-Image $item.defaultProperties.ItemIcon.ResourceObject.ObjectPath)}
}
foreach($r in $recipes.Values){$r.name=$items[$r.itemId].name}
# Display spelling confirmed by the player's 2026-10-26 station screenshots.
# Keep the native identity unchanged despite its normalized text typo.
if($recipes.Contains('/Game/Chimera/Crafting/CR_QuantumTranqualizer')){$recipes['/Game/Chimera/Crafting/CR_QuantumTranqualizer'].name='Quantum Tranquilizer'}
$font=[IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $FontDataFile))
if($font.Length -lt 16 -or [BitConverter]::ToUInt32($font,0) -ne $font.Length-8 -or [BitConverter]::ToUInt32($font,$font.Length-4) -ne 0 -or [Text.Encoding]::ASCII.GetString($font,4,4) -ne 'OTTO'){throw 'Unsupported native font envelope.'}
[IO.File]::WriteAllBytes((Join-Path $OutputDirectory 'Nippo-Regular.otf'),$font[4..($font.Length-5)])
$pack=[ordered]@{
    gameHash='95f95eee0b1913b8716de7fa5bf19877a91abe86e63bf2b6f0a29bd48d1d76f1'
    recipes=$recipes;items=$items
    tick=(Export-Image '/Game/Chimera/UI/Research/Textures/T_SR_unlockedItemIcon_128x128.0')
    frame=(Export-Image '/Game/Chimera/UI/Research/RecipeTable/Textures/SR_recipeTableRecipeIcon_frame.0')
    background=(Export-Image '/Game/Chimera/UI/Research/RecipeTable/Textures/SR_recipeTableRecipeIcon_bkgd.0')
    font='Nippo-Regular.otf'
}
$pack | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'catalogue.json') -Encoding utf8NoBOM
$files=[ordered]@{}
Get-ChildItem -LiteralPath $OutputDirectory -File | ForEach-Object {$files[$_.Name]=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
[ordered]@{status='private-local-game-assets';sourceBuild=25646882;catalogueSha256=(Get-FileHash -LiteralPath $CatalogueFile).Hash.ToLowerInvariant();files=$files} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'provenance.json') -Encoding utf8NoBOM
Write-Output "Prepared $($recipes.Count) recipes, $($items.Count) item definitions and $($images.Count) images. Keep this pack local."
