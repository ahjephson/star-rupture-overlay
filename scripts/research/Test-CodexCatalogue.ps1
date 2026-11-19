param([Parameter(Mandatory)][string]$PrivateRoot)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'New-CodexCatalogue.ps1') -PrivateRoot $PrivateRoot
function Assert($Condition,[string]$Message){if(-not $Condition){throw $Message}}
$c=Get-Content (Join-Path $root 'catalogue.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100
Assert ($c.buildings.Count -eq @($modeBuildings.Values|Where-Object normalCatalogueCandidate).Count) 'Selected-root normal building coverage is incomplete'
$producerSet=[Collections.Generic.HashSet[string]]::new([string[]]@($c.buildings.id),[StringComparer]::Ordinal)
$itemSet=[Collections.Generic.HashSet[string]]::new([string[]]@($c.items.id),[StringComparer]::Ordinal)
Assert ($itemSet.Count -eq $c.items.Count -and $producerSet.Count -eq $c.buildings.Count) 'Duplicate normalized identities'
$comparisons=0;$emptyDefaults=0
foreach($r in $c.recipes){
    $raw=(Complete $r.id 'CrItemRecipeData').properties
    Assert ($raw.UnlockType -cne 'ECrRecipeUnlockType::DevelopmentOnly') 'Development-only recipe was promoted'
    Assert ($r.output.itemId -ceq $raw.OutputItem.Item.Split('.')[0] -and $r.output.quantity -eq $raw.OutputItem.Count -and $r.output.quantity -gt 0) 'Output differs from raw source'
    Assert ($itemSet.Contains($r.output.itemId)) 'Missing output item'
    foreach($spec in @(@{field='NeededResources';bill=$r.inputs;item='Item';count='Count'},@{field='UnlockRequirements';bill=$r.unlock.research;item='Key';count='Value'})){
        if($raw.ContainsKey($spec.field)){
            Assert ($spec.bill.status -eq 'serialized' -and $spec.bill.items.Count -eq $raw[$spec.field].Count) 'Serialized bill lost rows'
            for($i=0;$i -lt $spec.bill.items.Count;$i++){
                $actual=$spec.bill.items[$i];$expected=$raw[$spec.field][$i]
                Assert ($actual.itemId -ceq $expected[$spec.item].Split('.')[0] -and $actual.quantity -eq $expected[$spec.count] -and $actual.quantity -gt 0 -and $itemSet.Contains($actual.itemId)) 'Bill row differs from source or references missing item'
            }
        }else{Assert ($spec.bill.status -eq 'matching native empty default' -and $spec.bill.items.Count -eq 0) 'Omitted bill must use proven native empty default';$emptyDefaults++}
    }
    $time=if($raw.ContainsKey('BuildTime')){$raw.BuildTime}else{$native.nativeDefaults.CrItemRecipeData.fields.BuildTime.value}
    Assert ($r.baseDurationSeconds.value -eq $time -and [double]::IsFinite($time)) 'Recipe base time differs from source/default'
    foreach($producer in $r.producerIds){Assert ($producerSet.Contains($producer) -and $r.id -cin @($c.buildings|Where-Object {$_.id -ceq $producer})[0].recipeIds) 'Producer link is not native collection membership'}
    $comparisons++
}
foreach($b in $c.buildings){
    $raw=(Complete $b.id 'CrBuildingData').properties
    Assert ($modeBuildings[$b.id].normalCatalogueCandidate) 'Unrooted/Test building was promoted'
    Assert ($b.construction.status -eq 'serialized' -and $b.construction.items.Count -eq $raw.ResourceRequirements.Requirements.Count) 'Construction bill incomplete'
    for($i=0;$i -lt $b.construction.items.Count;$i++){$a=$b.construction.items[$i];$v=$raw.ResourceRequirements.Requirements[$i];Assert ($a.itemId -ceq $v.Item.Split('.')[0] -and $a.quantity -eq $v.Quantity -and $itemSet.Contains($a.itemId)) 'Construction differs from source'}
    foreach($route in @($b.unlockRoutes|Where-Object type -eq tutorialObjectiveEvent)){
        $flow=$records[$route.flowId];$listeners=@($flow.exports|Where-Object {$_.objectPath -ceq $route.listener});$events=@($flow.exports|Where-Object {$_.objectPath -ceq $route.event})
        Assert ($listeners.Count -eq 1 -and $events.Count -eq 1 -and $listeners[0].properties.Event -ceq $events[0].objectPath -and $events[0].properties.bAvailable -eq $true -and $b.id -cin $events[0].properties.BuildingDataArray.package) 'Tutorial event ownership/target join failed'
    }
}
$ammoChecks=0;$tierChecks=0
if($c.baseCoreProgression){
    $binding=$proof.baseCoreBinding
    Assert ($c.baseCoreProgression.id -ceq $binding.value.package -and $c.baseCoreProgression.nativeTierBinding -eq 'verified matching native settings reference') 'Core tier data is not selected by native settings'
    $rawCore=(Complete $c.baseCoreProgression.id 'CrBaseCoreDataAsset').properties.BaseCoreLevels
    Assert ($c.baseCoreProgression.tiers.Count -eq $rawCore.Count) 'Tier count differs from source'
    for($i=0;$i -lt $rawCore.Count;$i++){
        $actual=$c.baseCoreProgression.tiers[$i];$expected=$rawCore[$i]
        Assert ($actual.sourceIndex -eq $i -and $actual.coolingCapacity -eq $expected.CoolingCapacity -and $actual.upgradeMaterials.status -eq 'serialized' -and $actual.upgradeMaterials.items.Count -eq $expected.ItemsToUpgrade.Count) 'Core tier fields differ from source'
        for($j=0;$j -lt $expected.ItemsToUpgrade.Count;$j++){
            $a=$actual.upgradeMaterials.items[$j];$v=$expected.ItemsToUpgrade[$j]
            Assert ($a.itemId -ceq $v.Item.Split('.')[0] -and $a.quantity -eq $v.Count -and $itemSet.Contains($a.itemId)) 'Core upgrade material differs from source'
        }
        $tierChecks++
    }
}
foreach($r in @($c.recipes|Where-Object {$_.unlock.kind -eq 'ECrRecipeUnlockType::Ammo'})){
    Assert ($r.unlock.condition.condition -eq 'firstWeaponPurchase' -and $r.unlock.weaponPrerequisites -is [array] -and $r.unlock.weaponPrerequisites.Count -gt 0) 'Ammo first-purchase condition or array shape missing'
    foreach($weapon in $r.unlock.weaponPrerequisites){
        Assert ($weapon.weaponClass -cin $offerClasses -and $weapon.ammoItemId -ceq $r.output.itemId -and $weapon.name) 'Ammo references an unregistered, unnamed or mismatched weapon'
        $cdos=@($records[$weapon.weaponId].exports|Where-Object {$_.schema -eq 'CrWeaponItemDataBase' -and ($_.flags -band 16)})
        Assert ($cdos.Count -eq 1 -and -not $cdos[0].error -and $cdos[0].completeObjectTail -and $cdos[0].properties.AmmoItem.Split('.')[0] -ceq $weapon.ammoItemId) 'Ammo weapon link is not from a complete CDO'
    }
    $ammoChecks++
}
foreach($excluded in $c.excludedWeapons){Assert ($excluded.id -cnotin $c.recipes.unlock.weaponPrerequisites.weaponId) 'Excluded weapon became an ammo prerequisite'}
Assert ($c.issues.Count -eq 0) 'Catalogue contains unresolved scoped fields; report before acceptance'
$icons=Get-Content (Join-Path $root 'catalogue-icons/manifest.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100
Assert ($icons.requested -eq $icons.decoded -and $icons.errors.Count -eq 0) 'Not all requested catalogue icons decoded'
foreach($file in $icons.icons.Values){$bytes=[IO.File]::ReadAllBytes((Join-Path $root $file));Assert ($bytes.Length -ge 8) 'Truncated icon header';$w=[BitConverter]::ToUInt32($bytes,0);$h=[BitConverter]::ToUInt32($bytes,4);Assert ($w -gt 0 -and $w -le 2048 -and $h -gt 0 -and $h -le 2048 -and $bytes.Length -eq 8+4*$w*$h) 'Invalid icon dimensions or payload'}
foreach($item in $c.items){Assert ($item.name -and $item.image.decoded) 'Required item name/icon unavailable'}
foreach($b in $c.buildings){Assert ($b.name -and $b.image.decoded) 'Required building name/icon unavailable'}
[ordered]@{status='passed';recipeSourceComparisons=$comparisons;buildingSourceComparisons=$c.buildings.Count;provenEmptyBillDefaults=$emptyDefaults;baseCoreTierComparisons=$tierChecks;ammoRecipeComparisons=$ammoChecks;icons=$icons.decoded;explicitIssues=$c.issues.Count;noUiImplemented=$true;noScreenCapture=$true}|ConvertTo-Json|Set-Content (Join-Path $root 'catalogue-validation.json') -Encoding utf8NoBOM
Get-Content (Join-Path $root 'catalogue-validation.json')