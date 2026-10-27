param([string]$RequestFile)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.Drawing
$request = Get-Content -LiteralPath $RequestFile -Raw -Encoding UTF8 | ConvertFrom-Json
$tools = $request.tools
$output = $request.output
$work = Join-Path $output 'work'
$utf8 = New-Object Text.UTF8Encoding($false)
New-Item -ItemType Directory -Force -Path $tools,$work | Out-Null
function Status([string]$message) { [IO.File]::WriteAllText($request.progress,$message,$utf8) }
function Hash([string]$path) {
    $algorithm=[Security.Cryptography.SHA256]::Create()
    $stream=[IO.File]::OpenRead($path)
    try { return ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace('-','').ToLowerInvariant() } finally { $stream.Dispose(); $algorithm.Dispose() }
}
function Download([string]$url,[string]$path,[string]$sha) {
    if ((Test-Path -LiteralPath $path) -and (Hash $path) -eq $sha) { return }
    $web=[Net.WebRequest]::Create($url)
    $web.Timeout=30000; $web.ReadWriteTimeout=30000
    $response=$web.GetResponse()
    try {
        if ($response.ContentLength -gt 64MB) { throw 'Setup download exceeds size bound.' }
        $input=$response.GetResponseStream(); $target=[IO.File]::Create($path+'.part')
        try {
            $buffer=New-Object byte[] 65536; $total=0L
            while (($count=$input.Read($buffer,0,$buffer.Length)) -gt 0) {
                $total += $count
                if ($total -gt 64MB) { throw 'Setup download exceeds size bound.' }
                $target.Write($buffer,0,$count)
            }
        } finally { $target.Dispose(); $input.Dispose() }
    } finally { $response.Dispose() }
    if ((Get-Item -LiteralPath ($path+'.part')).Length -gt 64MB -or (Hash ($path+'.part')) -ne $sha) { throw 'Setup download verification failed.' }
    Move-Item -LiteralPath ($path+'.part') -Destination $path -Force
}
function Entry([string]$archive,[string]$entry,[string]$destination,[string]$sha) {
    if ((Test-Path -LiteralPath $destination) -and (Hash $destination) -eq $sha) { return }
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $match = @($zip.Entries | Where-Object FullName -CEQ $entry)
        if ($match.Count -ne 1 -or $match[0].Length -gt 128MB) { throw 'Invalid setup archive entry.' }
        [IO.Compression.ZipFileExtensions]::ExtractToFile($match[0],$destination+'.part',$true)
    } finally { $zip.Dispose() }
    if ((Hash ($destination+'.part')) -ne $sha) { throw 'Setup component verification failed.' }
    Move-Item -LiteralPath ($destination+'.part') -Destination $destination -Force
}
Status 'Preparing recipe tracker: checking setup components...'
$uas = Join-Path $tools 'uas.exe'
$uasHash = '1cb1294459b9f9a261357c1944d7219e665de10727103b3b9b4f2fc3dce6c792'
if (-not (Test-Path -LiteralPath $uas) -or (Hash $uas) -ne $uasHash) {
    Status 'Preparing recipe tracker: downloading setup components...'
    $archive = Join-Path $tools 'uas-v0.5.0.zip'
    Download 'https://github.com/AndrewSav/UnrealAssetScout/releases/download/v0.5.0/UnrealAssetScout-v0.5.0-self-contained.zip' $archive '6af6e1ca983833b88fb767a1d5b89dd8037167a467cf58bbbb0113f9ca69443d'
    Entry $archive 'uas.exe' $uas $uasHash
}
$oodle = Join-Path $tools 'oodle-data-shared.dll'
$oodleHash = 'cba19529d0a3b5ec9c630e95652af01e123ae29a34a8a5f7507f5bcf23d9e82b'
if (-not (Test-Path -LiteralPath $oodle) -or (Hash $oodle) -ne $oodleHash) {
    Status 'Preparing recipe tracker: downloading decompressor...'
    $archive = Join-Path $tools 'oodle-2.9.16.zip'
    Download 'https://github.com/WorkingRobot/OodleUE/releases/download/2026-06-04-1357/clang-cl-x64-release.zip' $archive '37f816a3e8f5a65a11f6defc8c28c0508287602767741ba4803c6a21a62a877b'
    Entry $archive 'bin/oodle-data-shared.dll' $oodle $oodleHash
}
$zlib = Join-Path $tools 'zlib-ng2.dll'
$zlibHash = '454be2f3d10f804ace577198401431db5e95d0286b59589bc28a40085388e7c2'
if (-not (Test-Path -LiteralPath $zlib) -or (Hash $zlib) -ne $zlibHash) {
    Status 'Preparing recipe tracker: downloading decompressor...'
    $archive = Join-Path $tools 'zlib-ng2.dll.gz'
    Download 'https://github.com/NotOfficer/Zlib-ng.NET/releases/download/1.0.0/zlib-ng2.dll.gz' $archive 'e11f814f64821c482fb81c05fb20d9e5a3be0feb0e2f8fd9480d1f341163e2c1'
    $source = [IO.File]::OpenRead($archive)
    $gzip = New-Object IO.Compression.GZipStream($source,[IO.Compression.CompressionMode]::Decompress)
    $dest = [IO.File]::Create($zlib+'.part')
    try { $gzip.CopyTo($dest) } finally { $dest.Dispose(); $gzip.Dispose(); $source.Dispose() }
    if ((Hash ($zlib+'.part')) -ne $zlibHash) { throw 'Setup component verification failed.' }
    Move-Item -LiteralPath ($zlib+'.part') -Destination $zlib -Force
}
$detex = Join-Path $tools 'Detex.dll'
if ((Test-Path -LiteralPath $detex) -and (Hash $detex) -ne 'a434f5759fadb73bf386e636481e4fddb29e6a2e9c177334c53770a7aba19a7e') { throw 'Texture decoder verification failed.' }
function Export([string]$mode,[string]$filter,[string]$directory) {
    & $uas export $mode --paks $request.paks --game GAME_UE5_6 --usmap $request.mappings --filter $filter --output $directory --log (Join-Path $work ($mode+'.log')) *> (Join-Path $work ($mode+'.console.log'))
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the installed game assets.' }
    if ((Hash $detex) -ne 'a434f5759fadb73bf386e636481e4fddb29e6a2e9c177334c53770a7aba19a7e') { throw 'Texture decoder verification failed.' }
    $manifest = Get-Content -LiteralPath (Join-Path $directory '.uas-manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($entry in $manifest.sources.PSObject.Properties) { if ($entry.Value.s -ne 'ok') { throw 'A required game asset could not be read.' } }
}
Status 'Preparing recipe tracker: reading recipes and item definitions...'
$definitions = Join-Path $work 'definitions'
Export 'json' '^StarRupture/Content/Chimera/(Crafting/(?:[^/]+/)*CR_[^/]+|Items/I_[^/]+|Corporations/I_DataPoint)\.uasset$' $definitions
$rawItems = @{}
$rawRecipes = @{}
Get-ChildItem -LiteralPath $definitions -Recurse -File -Filter '*.json' | Where-Object Name -NotLike '.*' | ForEach-Object {
    if ($_.Length -gt 8MB) { throw 'Game definition exceeds size bound.' }
    $objects = Get-Content -LiteralPath $_.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($object in $objects) {
        if ($object.Type -eq 'CrItemRecipeData' -and $object.Properties.UnlockType -eq 'ECrRecipeUnlockType::RecipeTable') { $rawRecipes[$object.Package] = $object.Properties }
        if ($object.Name -like 'Default__I_*' -and $object.Package) { $rawItems[$object.Package] = $object.Properties }
    }
}
function ItemPath([string]$reference) {
    if ($reference -match "'(/Game/[^']+)'$") { $reference = $Matches[1] }
    $reference = $reference -replace '\.[^/\.]+$',''
    if ($reference -notmatch '^/Game/Chimera/[A-Za-z0-9_/-]+$') { throw 'Invalid game item reference.' }
    return $reference
}
$recipes = [ordered]@{}
$wanted = @{}
foreach ($id in ($rawRecipes.Keys | Sort-Object)) {
    $r = $rawRecipes[$id]
    $requirements = [ordered]@{}
    foreach ($payment in $r.UnlockRequirements) {
        $item = ItemPath $payment.Key
        if ($requirements.Contains($item) -or [long]$payment.Value -le 0 -or [long]$payment.Value -gt [uint32]::MaxValue) { throw 'Invalid research requirement.' }
        $requirements[$item] = [uint32]$payment.Value
        $wanted[$item] = $true
    }
    $product = ItemPath $r.OutputItem.Item.ObjectPath
    $wanted[$product] = $true
    $recipes[$id] = [ordered]@{ name=''; itemId=$product; requirements=$requirements }
}
if ($recipes.Count -ne 122) { throw 'No supported recipe definitions were found.' }
$iconPaths = @{}
$items = [ordered]@{}
foreach ($id in ($wanted.Keys | Sort-Object)) {
    if (-not $rawItems.ContainsKey($id)) { throw 'A required item definition is missing.' }
    $item = $rawItems[$id]
    $icon = ItemPath $item.ItemIcon.ResourceObject.ObjectPath
    $iconPaths[$icon] = $true
    $items[$id] = [ordered]@{ name=[string]$item.ItemName.LocalizedString; image=$icon }
    if (-not $items[$id].name) { throw 'A required item name is missing.' }
}
foreach ($recipe in $recipes.Values) { $recipe.name = $items[$recipe.itemId].name }
if ($recipes.Contains('/Game/Chimera/Crafting/CR_QuantumTranqualizer')) { $recipes['/Game/Chimera/Crafting/CR_QuantumTranqualizer'].name = 'Quantum Tranquilizer' }
$tick = '/Game/Chimera/UI/Research/Textures/T_SR_unlockedItemIcon_128x128'
$frame = '/Game/Chimera/UI/Research/RecipeTable/Textures/SR_recipeTableRecipeIcon_frame'
$background = '/Game/Chimera/UI/Research/RecipeTable/Textures/SR_recipeTableRecipeIcon_bkgd'
foreach ($icon in @($tick,$frame,$background)) { $iconPaths[$icon] = $true }
$pattern = '^(' + (($iconPaths.Keys | Sort-Object | ForEach-Object { [regex]::Escape('StarRupture/Content/'+$_.Substring(6)+'.uasset') }) -join '|') + ')$'
Status 'Preparing recipe tracker: reading icons...'
$textures = Join-Path $work 'textures'
Export 'textures' $pattern $textures
$images = @{}
$total = 0L
foreach ($icon in $iconPaths.Keys) {
    $source = Join-Path $textures ('StarRupture/Content/'+$icon.Substring(6)+'.png')
    $bitmap = [Drawing.Bitmap]::FromFile($source)
    try {
        $w=$bitmap.Width; $h=$bitmap.Height
        if ($w -lt 1 -or $h -lt 1 -or $w -gt 1024 -or $h -gt 1024) { throw 'Icon dimensions exceed bound.' }
        $total += $w*$h*4
        if ($total -gt 64MB) { throw 'Icon pack exceeds size bound.' }
        $rect=New-Object Drawing.Rectangle(0,0,$w,$h)
        $data=$bitmap.LockBits($rect,[Drawing.Imaging.ImageLockMode]::ReadOnly,[Drawing.Imaging.PixelFormat]::Format32bppPArgb)
        try {
            $pixels=New-Object byte[] ($w*$h*4)
            for($y=0;$y -lt $h;$y++){[Runtime.InteropServices.Marshal]::Copy([IntPtr]::Add($data.Scan0,$y*$data.Stride),$pixels,$y*$w*4,$w*4)}
            $hashAlgorithm=[Security.Cryptography.SHA256]::Create()
            try { $name=([BitConverter]::ToString($hashAlgorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($icon)))).Replace('-','').ToLowerInvariant().Substring(0,24)+'.bgra' } finally { $hashAlgorithm.Dispose() }
            $stream=[IO.File]::Create((Join-Path $output $name))
            try { $stream.Write([BitConverter]::GetBytes([uint32]$w),0,4); $stream.Write([BitConverter]::GetBytes([uint32]$h),0,4); $stream.Write($pixels,0,$pixels.Length) } finally { $stream.Dispose() }
            $images[$icon]=$name
        } finally { $bitmap.UnlockBits($data) }
    } finally { $bitmap.Dispose() }
}
Status 'Preparing recipe tracker: reading font...'
$fonts=Join-Path $work 'font'
Export 'raw' '/Nippo-Regular\.ufont$' $fonts
$fontFiles=@(Get-ChildItem -LiteralPath $fonts -Recurse -Filter '*.ufont' -File)
if ($fontFiles.Count -ne 1 -or $fontFiles[0].Length -gt 2MB) { throw 'Required game font is missing.' }
$font=[IO.File]::ReadAllBytes($fontFiles[0].FullName)
if($font.Length -lt 16 -or [BitConverter]::ToUInt32($font,0) -ne $font.Length-8 -or [BitConverter]::ToUInt32($font,$font.Length-4) -ne 0 -or [Text.Encoding]::ASCII.GetString($font,4,4) -ne 'OTTO') { throw 'Unsupported game font format.' }
[IO.File]::WriteAllBytes((Join-Path $output 'Nippo-Regular.otf'),$font[4..($font.Length-5)])
foreach ($item in $items.Values) { $item.image=$images[$item.image] }
$pack=[ordered]@{ gameHash=$request.gameHash; recipes=$recipes; items=$items; tick=$images[$tick]; frame=$images[$frame]; background=$images[$background]; font='Nippo-Regular.otf' }
[IO.File]::WriteAllText((Join-Path $output 'catalogue.json'),($pack | ConvertTo-Json -Depth 16),$utf8)
Status 'Recipe tracker assets are ready.'
