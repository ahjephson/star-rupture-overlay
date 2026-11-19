# Sourced only by the private catalogue normalizer after its bounded evidence helpers.
$weaponLinks=@{};$coreProgression=$null;$excludedWeapons=@()
$unlockManifestPath=Join-Path $root 'unlock-evidence/manifest.json'
if(Test-Path -LiteralPath $unlockManifestPath){
    $unlockManifest=Get-Content $unlockManifestPath -Raw|ConvertFrom-Json -AsHashtable -Depth 100
    $rawPath=Join-Path $root 'unlock-evidence/raw-assets.json'
    $proofPath=Join-Path $root 'unlock-evidence/native-evidence.json'
    if($unlockManifest.gameSha256 -cne $source.gameHash -or $unlockManifest.selectedMode -cne $native.selectedMode.class.path -or (Get-FileHash $rawPath).Hash.ToLowerInvariant() -cne $unlockManifest.rawSha256 -or (Get-FileHash $proofPath).Hash.ToLowerInvariant() -cne $unlockManifest.nativeSha256){throw 'Unlock evidence provenance mismatch'}
    $proof=Get-Content $proofPath -Raw|ConvertFrom-Json -AsHashtable -Depth 100
    if($proof.identity.executable_sha256 -cne $source.gameHash -or $proof.baseCoreBinding.value.package -cne $unlockManifest.coreAsset -or -not $proof.readOnly -or $proof.gameCodeInvoked){throw 'Unlock binding proof mismatch'}
    $offerClasses=@($proof.weaponOfferEvidence.weapons.objects.path|Sort-Object -Unique)
    if(($offerClasses -join "`n") -cne ($unlockManifest.offeredWeaponClasses -join "`n")){throw 'Offer proof differs from extraction manifest'}
    foreach($row in (Get-Content $rawPath -Raw|ConvertFrom-Json -AsHashtable -Depth 100)){
        $records[$row.asset]=$row;$origins[$row.asset]='unlock-evidence/raw-assets.json'
        foreach($e in $row.exports){if($e.schema -eq 'StringTable' -and $e.extra.entries){$tables["$($row.asset).$($e.name)"]=$e.extra.entries}}
    }
    $coreExport=Complete $unlockManifest.coreAsset 'CrBaseCoreDataAsset'
    $levels=@($coreExport.properties.BaseCoreLevels)
    if(-not $levels.Count -or $levels.Count -gt 64){throw 'Invalid Base Core tier count'}
    $tiers=@(for($i=0;$i -lt $levels.Count;$i++){
        $level=$levels[$i]
        [ordered]@{sourceIndex=$i;previousSourceIndex=$(if($i -gt 0){$i-1}else{$null});upgradeMaterials=(Bill $level 'ItemsToUpgrade' 'Item' 'Count' $unlockManifest.coreAsset);coolingCapacity=$level.CoolingCapacity}
    })
    $coreProgression=[ordered]@{id=$unlockManifest.coreAsset;nativeTierBinding='verified matching native settings reference';source='unlock-evidence/raw-assets.json';bindingSource='unlock-evidence/native-evidence.json';sequence=$confirmedRules.baseCore;sequenceEvidence='player-confirmed; native execution not inferred';tiers=$tiers;displayLevelNumbers='not inferred from source array indices';initialPlacementAvailability='separate from upgrade progression'}
    foreach($row in @($records.Values|Where-Object {$_.exports.schema -contains 'CrWeaponItemDataBase'})){
        $cdos=@($row.exports|Where-Object {$_.schema -eq 'CrWeaponItemDataBase' -and ($_.flags -band 16)})
        if($cdos.Count -ne 1){continue}
        $e=$cdos[0];$classPath=$row.asset+'.'+($e.name -replace '^Default__','')
        if($classPath -cnotin $offerClasses){$excludedWeapons += [ordered]@{id=$row.asset;reason='not in the observed registered purchase candidate list; not proof of global unavailability'};continue}
        if($e.error -or -not $e.completeObjectTail){Issue $row.asset 'weaponDefinition' 'Registered weapon CDO not completely decoded';continue}
        if(-not $e.properties.AmmoItem){continue}
        $ammo=Asset-Id $e.properties.AmmoItem
        if(-not $weaponLinks.ContainsKey($ammo)){$weaponLinks[$ammo]=@()}
        $weaponLinks[$ammo]+=[ordered]@{weaponId=$row.asset;weaponClass=$classPath;name=(Label $e.properties.ItemName $row.asset 'ItemName');ammoItemId=$ammo;source=$origins[$row.asset];eligibilitySource='unlock-evidence/native-evidence.json';scope='positive observed purchase candidate; current ownership not inferred'}
    }
}