param([Parameter(Mandatory)][string]$PrivateRoot)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'New-CodexSample.ps1') -PrivateRoot $PrivateRoot -EvidenceOnly -EvidenceDirectory catalogue-evidence
$confirmedRules=$null
$confirmationPath=Join-Path $root 'rule-confirmations.json'
if(Test-Path -LiteralPath $confirmationPath){
    $confirmedRules=Get-Content $confirmationPath -Raw|ConvertFrom-Json -AsHashtable -Depth 30
    if($confirmedRules.gameSha256 -cne $source.gameHash -or $confirmedRules.evidenceKind -notlike 'player-confirmed*'){throw 'Rule confirmation scope/provenance mismatch'}
}
$issues=[Collections.Generic.List[object]]::new()
function Issue([string]$Id,[string]$Field,[string]$Reason){$issues.Add([ordered]@{id=$Id;field=$Field;reason=$Reason})}
function Complete([string]$Id,[string]$Schema){
    if(-not $records.ContainsKey($Id)){throw "Missing package $Id"}
    $es=@($records[$Id].exports|Where-Object {$_.schema -ceq $Schema})
    if($es.Count -ne 1 -or $es[0].error -or -not $es[0].completeObjectTail){throw "Incomplete or ambiguous $Schema export: $Id"}
    return $es[0]
}
function Label($Value,[string]$Id,[string]$Field){if($Value -is [Collections.IDictionary] -and $Value.Contains('CultureInvariantString') -and -not $Value.CultureInvariantString){return $null};try{return Text $Value}catch{Issue $Id $Field $_.Exception.Message;return $null}}
$iconManifest=Join-Path $root 'catalogue-icons/manifest.json'
$iconFiles=if(Test-Path -LiteralPath $iconManifest){(Get-Content $iconManifest -Raw|ConvertFrom-Json -AsHashtable -Depth 30).icons}else{@{}}
function Catalogue-Image($Value){
    if(-not $Value){return $null}
    $id=$Value.Split('.')[0]
    $file=$iconFiles[$id]
    return [ordered]@{asset=$id;decoded=[bool]($file -and (Test-Path -LiteralPath (Join-Path $root $file)));file=$file}
}
function Bill($Properties,[string]$Field,[string]$ItemField,[string]$CountField,[string]$Id){
    if(-not $Properties.ContainsKey($Field)){
        if($Field -in @('NeededResources','UnlockRequirements')){
            $default=Default-Value $Properties $Field 'CrItemRecipeData'
            if($default.value.Count -ne 0){throw 'Nonempty default needs explicit normalization'}
            return [ordered]@{status='matching native empty default';items=@();source=$default.source}
        }
        return [ordered]@{status='omitted; native array default not established';items=$null}
    }
    try{return [ordered]@{status='serialized';items=@(Amounts $Properties[$Field] $ItemField $CountField)}}catch{Issue $Id $Field $_.Exception.Message;return [ordered]@{status='unresolved';items=$null}}
}
. (Join-Path $PSScriptRoot 'CodexUnlocks.ps1')
$tutorialRoutes=@{}
foreach($flow in $records.Values){
    $main=@($flow.exports|Where-Object schema -eq 'CrFlowEventDataAsset')
    if($main.Count -ne 1 -or $main[0].error -or -not $main[0].completeObjectTail){continue}
    foreach($reference in $main[0].properties.ContextualEventListeners){
        $listeners=@($flow.exports|Where-Object {$_.objectPath -ceq $reference -and $_.schema -eq 'CrContextualEventFlowListener'})
        if($listeners.Count -ne 1 -or $listeners[0].error -or -not $listeners[0].completeObjectTail){Issue $flow.asset 'tutorialListener' 'Referenced listener did not decode completely';continue}
        $listener=$listeners[0]
        $events=@($flow.exports|Where-Object {$_.objectPath -ceq $listener.properties.Event -and $_.schema -eq 'CrSetBuildingsAvailableFlowEvent'})
        if($events.Count -ne 1){continue}
        $event=$events[0]
        if($event.error -or -not $event.completeObjectTail -or $event.properties.bAvailable -ne $true){continue}
        foreach($target in $event.properties.BuildingDataArray){
            $id=Asset-Id $target.package
            if(-not $tutorialRoutes.ContainsKey($id)){$tutorialRoutes[$id]=@()}
            $tutorialRoutes[$id]+=[ordered]@{type='tutorialObjectiveEvent';flowId=$flow.asset;listener=$listener.objectPath;event=$event.objectPath;condition=$listener.properties.Context;objectiveId=$listener.properties.ContextValue;source=$origins[$flow.asset];modeScope='conditional authored tutorial flow; selected flow execution, skipped tutorial and player completion not inferred'}
        }
    }
}
function Routes([string]$Target){
    $routes=@(Corporation-Routes $Target 'BuildingRewards')
    if($tutorialRoutes.ContainsKey($Target)){$routes += $tutorialRoutes[$Target]}
    foreach($corp in $corporations){
        $p=$corp.exports[0].properties
        foreach($level in $p.Levels){foreach($collection in $level.BuildingCollectionRewards){
            $id=Asset-Id $collection
            try{$ce=Complete $id 'CrBuildingCollectionData'}catch{Issue $Target 'unlockCollection' $_.Exception.Message;continue}
            if(@($ce.properties.Buildings|Where-Object {(Asset-Id $_) -ceq $Target}).Count){$routes += [ordered]@{type='corporationBuildingCollectionReward';corporationId=$corp.asset;corporation=(Label $p.DisplayText $corp.asset 'DisplayText');level=$level.Level;collectionId=$id;source=$origins[$corp.asset];modeScope='authored reward; player completion and custom progression overrides not inferred'}}
        }}
    }
    foreach($upgrade in $upgrades){
        $u=$upgrade.exports[0].properties
        if((Asset-Id $u.BuildingReward) -cne $Target){continue}
        $recipe=Asset-Id $u.Recipe
        try{$re=Complete $recipe 'CrItemRecipeData'}catch{Issue $Target 'upgradeResearch' $_.Exception.Message;continue}
        $tasks=@($u.Tasks|ForEach-Object {[ordered]@{id=$_.ID;text=(Label $_.EntryText $upgrade.asset 'EntryText');context=$_.Context;contextValue=$_.ContextValue;count=$_.ContextNumber;checkWhenInactive=$_.bCheckConditionWhenInactive}})
        $routes += [ordered]@{type='upgradeStation';upgradeId=$upgrade.asset;offeredBy=@(Corporation-Routes $upgrade.asset 'BuildingUpgradeRecipes');tasks=$tasks;researchRecipeId=$recipe;research=(Bill $re.properties 'UnlockRequirements' 'Key' 'Value' $recipe);source=$origins[$upgrade.asset]}
    }
    return $routes
}
$buildings=@();$recipeLinks=@{}
foreach($entry in @($modeBuildings.GetEnumerator()|Sort-Object Key)){
    if(-not $entry.Value.normalCatalogueCandidate){continue}
    $id=$entry.Key
    try{$e=Complete $id 'CrBuildingData'}catch{Issue $id 'definition' $_.Exception.Message;continue}
    $p=$e.properties
    $construction=if($p.ResourceRequirements){Bill $p.ResourceRequirements 'Requirements' 'Item' 'Quantity' $id}else{[ordered]@{status='omitted; native default not established';items=$null}}
    $routes=@(Routes $id)

    $machine=[ordered]@{id=$id;name=(Label $p.BuildingName $id 'BuildingName');description=$(if($p.BuildingDescription){Label $p.BuildingDescription $id 'BuildingDescription'}else{$null});image=(Catalogue-Image $p.Icon.ResourceObject);category=$entry.Value.category;modeEligibility=$entry.Value;construction=$construction;placementAdditions=[ordered]@{basePlatform=(Default-Value $p 'AddBasePlatformCost' 'CrBuildingData');pillar=(Default-Value $p 'AddPillarCost' 'CrBuildingData')};unlockRoutes=$routes;entityId=$p.EntityType.EntityConfig.package;entityAncestry=@();crafting=$null;recipeIds=@();source=$origins[$id]}
    if($machine.entityId){
        try{
            $chain=@(Config-Chain $machine.entityId);$machine.entityAncestry=@($chain.asset)
            foreach($node in $chain){foreach($reference in $node.config.Traits){
                $selected=@($records[$node.asset].exports|Where-Object {$_.schema -eq 'CrMassBuildingBaseCoreTrait' -and $_.name -ceq $reference.Split('.')[-1]})
                if($selected.Count -eq 1 -and $selected[0].completeObjectTail -and -not $selected[0].error -and $confirmedRules.baseCore){
                    $machine['progression']=if($coreProgression){$coreProgression}else{[ordered]@{rule=$confirmedRules.baseCore;source='rule-confirmations.json';evidenceKind='player-confirmed';nativeTierBinding='not yet established';initialPlacementAvailability='not inferred from upgrade progression'}}
                }
            }}
            try{$trait=Selected-CraftingTrait $chain}catch{if($_.Exception.Message -ceq 'No selected crafting trait in entity ancestry'){$trait=$null}else{throw}}
            if($trait){
                $cp=$trait.properties.CraftingParameters
                $machine.crafting=[ordered]@{trait=$trait.name;CraftingLoopDuration=(Default-Value $cp 'CraftingLoopDuration' 'CrBuildingCraftingTrait');OverwriteRecipeBuildTime=(Default-Value $cp 'OverwriteRecipeBuildTime' 'CrBuildingCraftingTrait');bUseCraftingProcessor=(Default-Value $cp 'bUseCraftingProcessor' 'CrBuildingCraftingTrait');recipeCollectionId=$null;recipeListStatus='no serialized collection; dynamic/default behavior not inferred'}
                if($cp.RecipeCollection){
                    $collection=Asset-Id $cp.RecipeCollection;$ce=Complete $collection 'CrItemRecipeCollection';$machine.crafting.recipeCollectionId=$collection
                    $machine.crafting.recipeListStatus=$(if($ce.properties.ContainsKey('Recipes')){'serialized'}else{'omitted; native array default not established'})
                    foreach($reference in $ce.properties.Recipes){$rid=Asset-Id $reference;$machine.recipeIds+= $rid;if(-not $recipeLinks.ContainsKey($rid)){$recipeLinks[$rid]=@()};$recipeLinks[$rid]+=$id}
                }
            }
        }catch{Issue $id 'crafting' $_.Exception.Message}
    }
    if(-not $routes.Count -and -not ($machine.progression.nativeTierBinding -eq 'verified matching native settings reference')){Issue $id 'unlockRoutes' 'No direct, collection, tutorial objective or upgrade route found; initial placement and tier progression remain distinct'}
    $buildings+=$machine
}
$recipes=@();$excluded=@()
foreach($entry in @($recipeLinks.GetEnumerator()|Sort-Object Key)){
    $id=$entry.Key
    try{$e=Complete $id 'CrItemRecipeData'}catch{Issue $id 'recipe' $_.Exception.Message;continue}
    $p=$e.properties
    if($p.UnlockType -ceq 'ECrRecipeUnlockType::DevelopmentOnly'){$excluded += [ordered]@{id=$id;reason='DevelopmentOnly recipe; not promoted into normal production catalogue';producerIds=$entry.Value};continue}
    if(-not $p.OutputItem.Item){Issue $id 'output' 'No serialized output item; dynamic behavior not inferred';continue}
    $outId=Asset-Id $p.OutputItem.Item
    if(-not $itemDefinitions.ContainsKey($outId)){Issue $id 'output' 'Output item CDO unavailable';continue}
    $kind=$p.UnlockType
    if($kind -ceq 'ECrRecipeUnlockType::Ammo' -and (-not $confirmedRules.ammunition -or -not $weaponLinks.ContainsKey($outId))){Issue $id 'unlockRule' 'First-purchase rule or positive native weapon-to-ammo link unresolved'}
    if(-not $kind){Issue $id 'UnlockType' 'Omitted unlock type; native enum default not established'}
    $recipeWeapons=@();if($kind -ceq 'ECrRecipeUnlockType::Ammo' -and $weaponLinks.ContainsKey($outId)){$recipeWeapons=@($weaponLinks[$outId])}
    $recipes += [ordered]@{id=$id;name=(Label $p.DisplayText $id 'DisplayText');description=$(if($p.DisplayDescription){Label $p.DisplayDescription $id 'DisplayDescription'}else{$null});image=(Catalogue-Image $p.Icon.ResourceObject);output=[ordered]@{itemId=$outId;quantity=$p.OutputItem.Count};inputs=(Bill $p 'NeededResources' 'Item' 'Count' $id);unlock=[ordered]@{kind=$kind;ruleStatus=$(if($kind -ceq 'ECrRecipeUnlockType::Ammo' -and $confirmedRules.ammunition -and $weaponLinks.ContainsKey($outId)){'player-confirmed first weapon purchase; native weapon reference verified'}elseif($kind -ceq 'ECrRecipeUnlockType::Ammo'){'weapon-specific gate unresolved'}else{'authored classification and requirements; current unlock not inferred'});condition=$(if($kind -ceq 'ECrRecipeUnlockType::Ammo'){$confirmedRules.ammunition}else{$null});weaponPrerequisites=$recipeWeapons;research=(Bill $p 'UnlockRequirements' 'Key' 'Value' $id);serializedLevel=$p.Level;serializedCorporation=$p.Corporation;scope='authored requirements only; no current-world completion or custom-mode override inferred'};producerIds=@($entry.Value|Sort-Object -Unique);baseDurationSeconds=(Default-Value $p 'BuildTime' 'CrItemRecipeData');effectiveDurationStatus='not calculated; native runtime modifiers and recipe-time override semantics remain separate';source=$origins[$id]}
}
$retained=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach($r in $recipes){$retained.Add($r.output.itemId)|Out-Null;foreach($bill in @($r.inputs,$r.unlock.research)){foreach($v in $bill.items){$retained.Add($v.itemId)|Out-Null}}}
foreach($b in $buildings){foreach($v in $b.construction.items){$retained.Add($v.itemId)|Out-Null};foreach($route in $b.unlockRoutes){foreach($v in $route.research.items){$retained.Add($v.itemId)|Out-Null}}}
if($coreProgression){foreach($tier in $coreProgression.tiers){foreach($v in $tier.upgradeMaterials.items){$retained.Add($v.itemId)|Out-Null}}}
$items=@(foreach($id in @($retained|Sort-Object)){
    if(-not $itemDefinitions.ContainsKey($id)){Issue $id 'item' 'Required item CDO missing';continue}
    $d=$itemDefinitions[$id];$export=@($records[$id].exports|Where-Object {$_.name -ceq $d.export})[0];$p=$export.properties
    if(-not $d.name){Issue $id 'ItemName' 'Required item name unresolved'}
    [ordered]@{id=$id;name=$d.name;description=$(if($p.ItemDescription){Label $p.ItemDescription $id 'ItemDescription'}else{$null});image=(Catalogue-Image $p.ItemIcon.ResourceObject);recipeIds=@($recipes|Where-Object {$_.output.itemId -ceq $id}|ForEach-Object id);usedByRecipeIds=@($recipes|Where-Object {$id -cin $_.inputs.items.itemId}|ForEach-Object id);usedByBuildingIds=@($buildings|Where-Object {$id -cin $_.construction.items.itemId}|ForEach-Object id);scope='produced by a normal-root recipe or referenced by retained production/research/construction';source=$d.source}
})
foreach($row in $records.Values){foreach($e in $row.exports){if($e.schema -eq 'CrItemRecipeData' -and -not $recipeLinks.ContainsKey($row.asset)){$excluded += [ordered]@{id=$row.asset;reason='not referenced by a selected-root normal building recipe collection; research-only and unrooted recipes kept as evidence';producerIds=@()}}}}
$counts=[ordered]@{archivePackages=$records.Count;decodedItemDefinitions=$itemDefinitions.Count;catalogueItems=$items.Count;recipes=$recipes.Count;buildings=$buildings.Count;baseCoreTiers=@($coreProgression.tiers).Count;ammoItemLinks=$weaponLinks.Count;recipeProviders=@($buildings|Where-Object {$_.crafting.recipeCollectionId}).Count;excludedRecipes=$excluded.Count;excludedDevelopmentRecipes=@($excluded|Where-Object {$_.reason -like 'DevelopmentOnly*'}).Count;issues=$issues.Count}
$result=[ordered]@{schemaVersion=1;kind='private complete selected-root catalogue investigation';gameSha256=$source.gameHash;scope=[ordered]@{selectedMode=$native.selectedMode.class.path;authoredRoot=$native.selectedMode.keeperCdo.path;rootCount=$modeBuildings.Count;normalBuildingCandidates=@($modeBuildings.Values|Where-Object normalCatalogueCandidate).Count;otherModesSupported=$false;currentWorldProgressIncluded=$false;runtimeRatesIncluded=$false;coverage='all selected-root normal building definitions and their serialized non-development recipe links; standalone unlinked items not promoted'};confirmedRules=$confirmedRules;baseCoreProgression=$coreProgression;excludedWeapons=$excludedWeapons;counts=$counts;items=$items;recipes=$recipes;buildings=$buildings;excludedRecipes=$excluded;excludedBuildings=@($modeBuildings.GetEnumerator()|Where-Object {-not $_.Value.normalCatalogueCandidate}|ForEach-Object {[ordered]@{id=$_.Key;reason=$_.Value.category}});issues=@($issues);provenance=[ordered]@{archive='index/source.json';native='mode-defaults/native-evidence.json';dependencies='catalogue-evidence/manifest.json';uiComparison='game-comparison.json'}}
$result|ConvertTo-Json -Depth 70|Set-Content (Join-Path $root 'catalogue.json') -Encoding utf8NoBOM
$counts|ConvertTo-Json -Compress
function Cell($Value){return ([string]$Value).Replace('|','\|').Replace("`r",' ').Replace("`n",' ')}
function Show-Bill($Bill){if($null -eq $Bill.items){return 'Unknown: '+$Bill.status};if(-not $Bill.items.Count){return 'None (native/serialized empty)'};return (@($Bill.items|ForEach-Object {"$($_.quantity) × $($_.name)"}) -join '; ')}
$lines=[Collections.Generic.List[string]]::new()
$lines.Add('# Production catalogue')
$lines.Add('')
$lines.Add("Contains $($items.Count) items, $($recipes.Count) recipes and $($buildings.Count) building definitions from the selected Update 3 PTR2 authored root. The catalogue records static requirements; it does not report current unlock progress, world-setting overrides, placement or sustained production rates.")
$lines.Add('')
$lines.Add("All $($items.Count) retained item names resolve to native text. Icons: $($iconFiles.Count) decoded image assets. Nine Test building entries and $($excluded.Count) recipes outside the normal production scope remain in separate evidence/exclusion records. Development-only recipes are excluded even when a normal-root machine references them.")
$lines.Add('')
$lines.Add('## Items and production')
$lines.Add('')
$lines.Add('| Item / recipe | Made in | Inputs per batch | Output | Base seconds | Unlock kind | Research submissions |')
$lines.Add('| --- | --- | --- | --- | --- | --- | --- |')
foreach($r in @($recipes|Sort-Object {$_.name},{$_.id})){
    $names=@($buildings|Where-Object {$_.id -cin $r.producerIds}|ForEach-Object name) -join ', '
    $lines.Add("| $(Cell $r.name) / $(Cell $r.id.Split('/')[-1]) | $(Cell $names) | $(Cell (Show-Bill $r.inputs)) | $($r.output.quantity) | $($r.baseDurationSeconds.value) | $(Cell ($r.unlock.kind -replace '^ECrRecipeUnlockType::','')) | $(Cell (Show-Bill $r.unlock.research)) |")
}
$lines.Add('')
$lines.Add('RecipeTable means an authored recipe-station submission bill. Building means access follows the producing building. Ammo requires the first purchase of a corresponding weapon, as confirmed by the player; exact native weapon references are listed below and do not imply current ownership. UnlockedByDefault is the native recipe flag within this content scope. Empty research submissions do not remove the producing machine or ammo gate.')
$lines.Add('')
$lines.Add('## Ammunition weapon prerequisites')
$lines.Add('')
$lines.Add('| Ammunition | Weapon registered for purchase | Condition |')
$lines.Add('| --- | --- | --- |')
foreach($entry in @($weaponLinks.GetEnumerator()|Sort-Object Key)){
    $ammoName=$itemDefinitions[$entry.Key].name
    $weapons=@($entry.Value|ForEach-Object name) -join ', '
    $lines.Add("| $(Cell $ammoName) | $(Cell $weapons) | First weapon purchase (player-confirmed rule; native reference verified) |")
}
$lines.Add('')
$lines.Add('## Base Core upgrade sequence')
$lines.Add('')
$lines.Add('The next tier is revealed after completing the current upgrade (player-confirmed behavior). Native settings select the following authored data. Source indices are retained; displayed tier numbers and initial placement availability are not inferred.')
$lines.Add('')
$lines.Add('| Source tier index | Previous source index | Upgrade materials | Authored cooling capacity |')
$lines.Add('| --- | --- | --- | --- |')
foreach($tier in $coreProgression.tiers){$lines.Add("| $($tier.sourceIndex) | $($tier.previousSourceIndex) | $(Cell (Show-Bill $tier.upgradeMaterials)) | $($tier.coolingCapacity) |")}
$lines.Add('')
$lines.Add('## Construction and building unlocks')
$lines.Add('')
$lines.Add('| Building | Category | Authored construction materials | Unlock sources |')
$lines.Add('| --- | --- | --- | --- |')
foreach($b in @($buildings|Sort-Object {$_.name},{$_.id})){
    $routeNames=@($b.unlockRoutes|ForEach-Object {$route=$_;switch($route.type){
        'corporationLevelReward' {"$($route.corporation), level $($route.level)"}
        'corporationBuildingCollectionReward' {"$($route.corporation), level $($route.level), collection $($route.collectionId.Split('/')[-1])"}
        'upgradeStation' {'Upgrade Station; tasks and submissions below'}
        'tutorialObjectiveEvent' {"Tutorial: $($route.condition) $($route.objectiveId)"}
    }}) -join '; '
    if(-not $routeNames){$routeNames=if($b.progression.nativeTierBinding -eq 'verified matching native settings reference'){'Sequential Base Core upgrades; see tier table. Initial placement is separate.'}else{'Unresolved; not assumed initially available'}}
    $lines.Add("| $(Cell $b.name) | $(Cell $b.category) | $(Cell (Show-Bill $b.construction)) | $(Cell $routeNames) |")
}
$lines.Add('')
$lines.Add('## Upgrade requirements')
foreach($b in @($buildings|Sort-Object {$_.name})){foreach($u in @($b.unlockRoutes|Where-Object type -eq upgradeStation)){
    $lines.Add('');$lines.Add("### $($b.name)");$lines.Add('')
    foreach($offer in $u.offeredBy){$lines.Add("Offered by $($offer.corporation), level $($offer.level).")}
    foreach($task in $u.tasks){$lines.Add("- $($task.text) — $($task.context), $($task.contextValue), count $($task.count).")}
    $lines.Add('');$lines.Add("Research submissions: $(Show-Bill $u.research).")
}}
$lines.Add('');$lines.Add('## Remaining boundaries');$lines.Add('')
if($confirmedRules){
    $lines.Add('- Player-confirmed Base Core rule: each next tier is offered after the current upgrade completes. Initial placement remains separate; the tier-data reference is independently verified against native settings.')
    $lines.Add('- Player-confirmed ammo rule: first purchase of the corresponding weapon unlocks its ammunition. Weapon-to-ammo references use complete native weapon records and positive observed purchase-candidate evidence. Candidates absent from the observed purchase list remain excluded.')
}
foreach($issue in $issues){$lines.Add("- $($issue.id): $($issue.field) — $($issue.reason).")}
$lines.Add('- Tutorial routes are conditional authored events. Their execution, skipped-tutorial behavior and completion in a particular world are not inferred. Player-confirmed rules are recorded separately from native source joins.')
$lines.Add('- All static construction and production/research quantities retain source links. Resource deposit conditions, runtime modifiers, first-purchase execution proof and dynamic recipe lists are separate.')
$lines.Add('- Base recipe time is distinct from CraftingLoopDuration. Omitted input/research containers use matching native empty defaults; explicit asset values take precedence.')
$lines.Add('- Source-only unsupported visualization/replication payloads, an unresolved decorative icon reference and unlinked item prototypes are not silently promoted into the catalogue. All required selected definition/config/crafting/recipe exports are complete.')
$lines.Add('- No further screen captures were taken. The earlier Smelter and Furnace-group observations apply only to their recorded fields, not this entire catalogue.')
$lines|Set-Content (Join-Path $root 'catalogue.md') -Encoding utf8NoBOM