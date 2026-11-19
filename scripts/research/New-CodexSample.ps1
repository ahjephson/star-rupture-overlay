param([Parameter(Mandatory)][string]$PrivateRoot,[switch]$EvidenceOnly,[string]$EvidenceDirectory)
$ErrorActionPreference = 'Stop'
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$root = (Resolve-Path -LiteralPath $PrivateRoot).Path
$local = (Resolve-Path -LiteralPath (Join-Path $project 'local')).Path
if (-not $root.StartsWith($local + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Sample output must stay in ignored local storage.' }
$records = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
$origins = @{}
$batchFiles = if ($EvidenceDirectory) {
    $manifest = Get-Content (Join-Path $root "$EvidenceDirectory/manifest.json") -Raw | ConvertFrom-Json -AsHashtable -Depth 100
    @($manifest.origins.Values | Sort-Object -Unique | ForEach-Object { "$EvidenceDirectory/$_" })
} else { @('recipes-final','producers-final','routes-final','collections-final','details','additional','additional-machines','missing-items','icons','parents') | ForEach-Object { "$_/raw-assets.json" } }
foreach ($relative in $batchFiles) {
    $file = Join-Path $root $relative
    if (-not (Test-Path -LiteralPath $file)) { throw "Missing research evidence: $relative" }
    foreach ($row in (Get-Content -LiteralPath $file -Raw | ConvertFrom-Json -AsHashtable -Depth 100)) {
        $records[$row.asset] = $row
        $origins[$row.asset] = $relative
    }
}
$source = Get-Content (Join-Path $root 'index/source.json') -Raw | ConvertFrom-Json -AsHashtable -Depth 100
$native = Get-Content (Join-Path $root 'mode-defaults/native-evidence.json') -Raw | ConvertFrom-Json -AsHashtable -Depth 100
if ($native.identity.executable_sha256 -cne $source.gameHash -or -not $native.readOnly -or $native.gameCodeInvoked) { throw 'Native evidence does not match the asset source.' }
$schemas = Get-Content (Join-Path $root 'index/schemas.json') -Raw | ConvertFrom-Json -AsHashtable -Depth 100
$categories = $schemas.enums.ECrBuildingUIType.entries
$modeBuildings = @{}
foreach ($entry in $native.selectedMode.categories) {
    $id = $entry.object.path.Split('.')[0]
    if ($modeBuildings.ContainsKey($id)) { throw 'Duplicate mode building identity' }
    $category = $categories[[string]$entry.UIType.value]
    if (-not $category) { throw 'Unmapped native building category' }
    $modeBuildings[$id] = [ordered]@{authoredRootMember=$true;category=$category;normalCatalogueCandidate=($category -notin @('Test','MAX','All','Favourites'));currentUnlock='not inferred';source='mode-defaults/native-evidence.json'}
}
function Default-Value($Properties, [string]$Field, [string]$Class) {
    if ($Properties.ContainsKey($Field)) { return [ordered]@{value=$Properties[$Field];origin='serialized asset'} }
    $default = $native.nativeDefaults[$Class]
    if (-not $default -or -not ($default.defaultObject.flags -band 16) -or -not $default.fields.ContainsKey($Field)) { throw "Unproven native default: $Class.$Field" }
    return [ordered]@{value=$default.fields[$Field].value;origin='matching executable native class default';source='mode-defaults/native-evidence.json';class=$Class;field=$Field}
}
function Config-Chain([string]$EntityId) {
    $chain = @(); $seen = @{}
    while ($EntityId) {
        if ($seen.ContainsKey($EntityId) -or $chain.Count -ge 16) { throw 'Cyclic or excessive entity ancestry' }
        $seen[$EntityId] = $true
        if (-not $records.ContainsKey($EntityId)) { throw "Entity parent not followed: $EntityId" }
        $row = $records[$EntityId]
        $configs = @($row.exports | Where-Object { $_.schema -in @('CrMassBuildingEntityConfigAsset','MassEntityConfigAsset') })
        if ($configs.Count -ne 1 -or $configs[0].error -or -not $configs[0].completeObjectTail) { throw 'Incomplete or ambiguous entity config' }
        $chain += [ordered]@{asset=$EntityId;config=$configs[0].properties.Config}
        $EntityId = if ($configs[0].properties.Config.Parent) { Asset-Id $configs[0].properties.Config.Parent } else { $null }
    }
    return $chain
}
function Selected-CraftingTrait($Chain) {
    foreach ($node in $Chain) {
        $selected = @()
        foreach ($reference in $node.config.Traits) {
            if ($reference -isnot [string]) { throw 'Invalid trait reference' }
            $parts = $reference.Split('.')
            if ($parts.Count -ne 2) { throw 'Invalid exported trait identity' }
            $package = if ($parts[0].StartsWith('/Game/')) { $parts[0] } else { $node.asset }
            if (-not $records.ContainsKey($package)) { throw 'Referenced trait package missing' }
            $exports = @($records[$package].exports | Where-Object { $_.name -ceq $parts[1] })
            if ($exports.Count -ne 1) { throw 'Ambiguous referenced trait export' }
            if ($exports[0].schema -eq 'CrBuildingCraftingTrait') { $selected += $exports[0] }
        }
        if ($selected.Count -gt 1) { throw 'Duplicate exact-type crafting traits' }
        if ($selected.Count -eq 1) {
            if ($selected[0].error -or -not $selected[0].completeObjectTail) { throw 'Incomplete selected crafting trait' }
            return $selected[0]
        }
    }
    throw 'No selected crafting trait in entity ancestry'
}
function Asset-Id($value) {
    if ($value -isnot [string] -or -not $value.StartsWith('/Game/Chimera/', [StringComparison]::Ordinal)) { throw "Invalid native asset reference: $value" }
    return $value.Split('.')[0]
}
$tables = @{}
foreach ($row in $records.Values) {
    foreach ($export in $row.exports) {
        if ($export.schema -eq 'StringTable' -and $export.extra.entries) { $tables["$($row.asset).$($export.name)"] = $export.extra.entries }
    }
}
function Text($value) {
    if ($value.SourceString) { return $value.SourceString }
    if ($value.CultureInvariantString) { return $value.CultureInvariantString }
    if ($value.TableId -and $value.Key -and $tables.ContainsKey($value.TableId) -and $tables[$value.TableId].Contains($value.Key)) { return $tables[$value.TableId][$value.Key] }
    throw 'Unresolved native display text'
}
function Image($value) {
    if (-not $value) { return $null }
    if (-not $value.StartsWith('/Game/Chimera/', [StringComparison]::Ordinal)) { return [ordered]@{asset=$value;decoded=$false;status='outside game icon namespace'} }
    $id = Asset-Id $value
    $e = if ($records.ContainsKey($id)) { $records[$id].exports[0] } else { $null }
    $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($id))).ToLowerInvariant().Substring(0,24)
    $relative = "icons/icons/$hash.bgra"
    return [ordered]@{asset=$id;decoded=(Test-Path -LiteralPath (Join-Path $root $relative));file=$relative;texture=$e.extra}
}
$itemDefinitions = @{}
foreach ($row in $records.Values) {
    foreach ($e in $row.exports) {
        if (($e.flags -band 16) -and $e.properties.ItemName -and -not $e.error) {
            if (-not $e.completeObjectTail) { throw "Item CDO not completely consumed: $($row.asset)" }
                        $itemName = try { Text $e.properties.ItemName } catch { if (-not $EvidenceOnly) { throw }; $null }
            $itemDefinitions[$row.asset] = [ordered]@{id=$row.asset;name=$itemName;nameStatus=$(if($itemName){'resolved'}else{'unresolved native text'});image=(Image $e.properties.ItemIcon.ResourceObject);source=$origins[$row.asset];export=$e.name}
        }
    }
}
function Amounts($rows, [string]$ItemField, [string]$CountField) {
    $out = @()
    foreach ($row in $rows) {
        $id = Asset-Id $row[$ItemField]
        if (-not $itemDefinitions.ContainsKey($id)) { throw "Missing referenced item definition: $id" }
        $count = $row[$CountField]
        if ($count -isnot [long] -and $count -isnot [int]) { throw "Non-integral item quantity: $id" }
        if ($count -le 0) { throw "Nonpositive item quantity: $id" }
        $out += [ordered]@{itemId=$id;name=$itemDefinitions[$id].name;quantity=$count}
    }
    return $out
}
$corporations = @($records.Values | Where-Object { $_.exports[0].schema -eq 'CrCorporationData' -and -not $_.exports[0].error })
$upgrades = @($records.Values | Where-Object { $_.exports[0].schema -eq 'CrBuildingUpgradeRecipeData' -and -not $_.exports[0].error })
function Corporation-Routes([string]$Target, [string]$RewardField) {
    $routes = @()
    foreach ($corp in $corporations) {
        $p = $corp.exports[0].properties
        for ($i=0; $i -lt $p.Levels.Count; $i++) {
            $level = $p.Levels[$i]
            foreach ($reward in $level[$RewardField]) {
                if ((Asset-Id $reward) -ceq $Target) {
                    $routes += [ordered]@{type='corporationLevelReward';corporationId=$corp.asset;corporation=(Text $p.DisplayText);level=$level.Level;source=$origins[$corp.asset];pointer="/exports/0/properties/Levels/$i/$RewardField";modeScope='authored corporation definition; current mode not inferred'}
                }
            }
        }
    }
    return $routes
}
if ($EvidenceOnly) { return }
$machines = @()
foreach ($row in @($records.Values | Where-Object { $_.exports[0].schema -eq 'CrBuildingData' } | Sort-Object asset)) {
    $e = $row.exports[0]; $p = $e.properties
    if ($e.error -or -not $e.completeObjectTail) { throw "Building data incomplete: $($row.asset)" }
    $entityId = $p.EntityType.EntityConfig.package
    if (-not $records.ContainsKey($entityId)) { throw "Entity definition not followed: $entityId" }
    $entity = $records[$entityId]
    $chain = @(Config-Chain $entityId)
    $trait = Selected-CraftingTrait $chain
    if (-not $modeBuildings.ContainsKey($row.asset) -or -not $modeBuildings[$row.asset].normalCatalogueCandidate) { throw 'Sample machine is outside the selected normal catalogue candidates' }
    $parameters = $trait.properties.CraftingParameters
    $collectionId = Asset-Id $parameters.RecipeCollection
    if (-not $records.ContainsKey($collectionId)) { throw "Recipe collection not followed: $collectionId" }
    $collection = $records[$collectionId].exports[0]
    if (-not $collection.completeObjectTail -or $collection.error) { throw 'Recipe collection incomplete' }
    $unlockRoutes = @(Corporation-Routes $row.asset 'BuildingRewards')
    foreach ($upgrade in $upgrades) {
        $u = $upgrade.exports[0].properties
        if ((Asset-Id $u.BuildingReward) -cne $row.asset) { continue }
        $researchId = Asset-Id $u.Recipe
        if (-not $records.ContainsKey($researchId)) { throw 'Upgrade research not followed' }
        $research = $records[$researchId].exports[0].properties
        $tasks = @($u.Tasks | ForEach-Object { [ordered]@{id=$_.ID;text=(Text $_.EntryText);context=$_.Context;contextValue=$_.ContextValue;count=$_.ContextNumber;checkWhenInactive=$_.bCheckConditionWhenInactive} })
        $unlockRoutes += [ordered]@{type='upgradeStation';upgradeId=$upgrade.asset;offeredBy=@(Corporation-Routes $upgrade.asset 'BuildingUpgradeRecipes');tasks=$tasks;researchRecipeId=$researchId;researchMaterials=@(Amounts $research.UnlockRequirements 'Key' 'Value');source=$origins[$upgrade.asset]}
    }
    $machines += [ordered]@{
        id=$row.asset;name=(Text $p.BuildingName);image=(Image $p.Icon.ResourceObject)
        authoredConstructionMaterials=@(Amounts $p.ResourceRequirements.Requirements 'Item' 'Quantity')
        placementAdditions=[ordered]@{basePlatform=(Default-Value $p 'AddBasePlatformCost' 'CrBuildingData');pillar=(Default-Value $p 'AddPillarCost' 'CrBuildingData');status='native omitted flags resolved; placement conditions and world modifiers remain separate'}
        modeEligibility=$modeBuildings[$row.asset];entityAncestry=@($chain.asset)
        unlockRoutes=$unlockRoutes;unlockCoverage=($(if($unlockRoutes.Count){'authored routes found; selected-mode root/category candidate verified; player completion not inferred'}else{'no direct route found in sampled reward sources; not proof of default availability'}))
        entityId=$entityId;craftingTrait=$trait.name;recipeCollectionId=$collectionId
        recipes=@($collection.properties.Recipes | ForEach-Object { Asset-Id $_ })
        craftingParameters=[ordered]@{CraftingLoopDuration=(Default-Value $parameters 'CraftingLoopDuration' 'CrBuildingCraftingTrait');OverwriteRecipeBuildTime=(Default-Value $parameters 'OverwriteRecipeBuildTime' 'CrBuildingCraftingTrait');bUseCraftingProcessor=(Default-Value $parameters 'bUseCraftingProcessor' 'CrBuildingCraftingTrait')};effectiveDurationStatus='not calculated; CraftingLoopDuration is not substituted for recipe BuildTime; runtime modifiers remain separate'
        source=[ordered]@{definition=$origins[$row.asset];entity=$origins[$entityId];collection=$origins[$collectionId]}
    }
}
$recipes = @()
$recipeBatch = Get-Content (Join-Path $root 'recipes-final/raw-assets.json') -Raw | ConvertFrom-Json -AsHashtable -Depth 100
foreach ($row in $recipeBatch) {
    $e = $row.exports[0]; $p = $e.properties
    if ($e.error -or -not $e.completeObjectTail) { throw "Recipe incomplete: $($row.asset)" }
    $outputId = Asset-Id $p.OutputItem.Item
    if (-not $itemDefinitions.ContainsKey($outputId)) { throw 'Recipe output item definition missing' }
    $producerIds = @($machines | Where-Object { $row.asset -cin $_.recipes } | ForEach-Object id)
    if (-not $producerIds.Count) { throw "No verified producer for sample recipe: $($row.asset)" }
    $recipes += [ordered]@{
        id=$row.asset;name=(Text $p.DisplayText);output=[ordered]@{itemId=$outputId;quantity=$p.OutputItem.Count}
        inputs=@(Amounts $p.NeededResources 'Item' 'Count')
        unlock=[ordered]@{kind=$p.UnlockType;researchMaterials=@(Amounts $p.UnlockRequirements 'Key' 'Value');serializedLevel=$p.Level;serializedCorporation=$p.Corporation;currentWorldStatus='not observed'}
        producerIds=$producerIds;image=(Image $p.Icon.ResourceObject)
        serializedBaseDurationSeconds=$p.BuildTime;baseDurationSeconds=(Default-Value $p 'BuildTime' 'CrItemRecipeData');effectiveDurationStatus='not a production-rate claim'
        source=[ordered]@{file='recipes-final/raw-assets.json';export=0;fields=@('NeededResources','OutputItem','UnlockType','UnlockRequirements','BuildTime')}
    }
}
$ids = @($recipes.output.itemId | Sort-Object -Unique)
$items = @($ids | ForEach-Object { $id=$_; [ordered]@{id=$id;name=$itemDefinitions[$id].name;image=$itemDefinitions[$id].image;recipeIds=@($recipes | Where-Object {$_.output.itemId -ceq $id} | ForEach-Object id)} })
$source = Get-Content (Join-Path $root 'index/source.json') -Raw|ConvertFrom-Json -AsHashtable -Depth 100
$result = [ordered]@{
    schemaVersion=1;kind='private authored-data feasibility sample';gameSha256=$source.gameHash;source='installed game archives, matching cached native schema, native string tables'
    scope=[ordered]@{currentWorldObserved=$false;allGameContentCovered=$false;selectedModeEligibility='authored selected-mode root plus normal category; not a player-unlock or placement claim';liveUnlockAndTaskProgress='not observed';fullPlacementBillDefaults='platform/pillar native flags resolved; actual placement conditions not inferred'}
    selectedMode=[ordered]@{mode=$native.selectedMode.class.path;state=$native.selectedMode.stateClass.path;keeper=$native.selectedMode.keeperClass.path;authoredBuildingCount=$modeBuildings.Count;normalCandidateCount=@($modeBuildings.Values|Where-Object normalCatalogueCandidate).Count;excludedTestCount=@($modeBuildings.Values|Where-Object {$_.category -eq 'Test'}).Count;rule='authored root and non-Test normal category; conservative catalogue scope, not runtime AvailableBuildings';source='mode-defaults/native-evidence.json'}
    items=$items;recipes=$recipes;machines=$machines;referencedItems=@($itemDefinitions.Values|Sort-Object id)
    relationships='BD.EntityType.EntityConfig -> DA.Config.Traits -> CrBuildingCraftingTrait.CraftingParameters.RecipeCollection -> CRC.Recipes -> CR.OutputItem'
}
$result|ConvertTo-Json -Depth 50|Set-Content -LiteralPath (Join-Path $root 'sample-catalogue.json') -Encoding utf8NoBOM
[ordered]@{items=$items.Count;recipes=$recipes.Count;machines=$machines.Count;referencedItems=$itemDefinitions.Count}|ConvertTo-Json -Compress

function Describe-Amounts($rows) {
    if (-not $rows.Count) { return '—' }
    return (($rows | ForEach-Object { "$($_.quantity) × $($_.name)" }) -join '; ')
}
$lines=[Collections.Generic.List[string]]::new()
$lines.Add('# Sample production catalogue')
$lines.Add('')
$lines.Add('Read-only investigation of installed Update 3 PTR2 assets. This is a private static-data sample, not a current-world unlock report, complete game catalogue or UI implementation.')
$lines.Add('')
$lines.Add("Contains $($items.Count) items, $($recipes.Count) production recipes, $($machines.Count) machine definitions and their linked images. Research materials, production materials, upgrade submissions and construction requirements are separate.")
$lines.Add('')
$lines.Add('## Selected mode and defaults')
$lines.Add('')
$lines.Add("Selected mode: $($native.selectedMode.class.path). Its state and technology-keeper CDO chain supplies $($modeBuildings.Count) authored references: $(@($modeBuildings.Values | Where-Object normalCatalogueCandidate).Count) normal catalogue candidates and $(@($modeBuildings.Values | Where-Object {$_.category -eq 'Test'}).Count) Test entries. This is content scope, not current unlock or placement availability.")
$lines.Add('')
$lines.Add('Native defaults: AddBasePlatformCost=false; AddPillarCost=false; recipe BuildTime=10 seconds; crafting CraftingLoopDuration=0; OverwriteRecipeBuildTime=-1; bUseCraftingProcessor=true. Explicit asset values take precedence. All 13 sampled recipes serialize their own BuildTime; none needs the 10-second fallback.')
$lines.Add('')
$lines.Add('## Production and research')
$lines.Add('')
$lines.Add('| Item / native recipe | Made in | Inputs per batch | Output per batch | Item unlock | Research submissions |')
$lines.Add('| --- | --- | --- | --- | --- | --- |')
foreach($r in $recipes) {
    $producerNames=(@($machines|Where-Object {$_.id -cin $r.producerIds}|ForEach-Object name) -join ', ')
    $unlock=$r.unlock.kind -replace '^ECrRecipeUnlockType::',''
    $research=Describe-Amounts $r.unlock.researchMaterials
    if($unlock -eq 'Building'){$research='Through producing-machine access; no independent recipe-table bill serialized'}
    $variant=$r.id.Split('/')[-1]
    $lines.Add("| $($r.name) / $variant | $producerNames | $(Describe-Amounts $r.inputs) | $($r.output.quantity) × $($r.name) | $unlock | $research |")
}
$lines.Add('')
$lines.Add('## Machines')
$lines.Add('')
$lines.Add('Construction amounts below are authored requirements. Platform/pillar flags use explicit asset values first, then defaults read from the matching executable. Placement conditions and world modifiers remain separate.')
foreach($m in $machines) {
    $lines.Add('')
    $lines.Add("### $($m.name)")
    $lines.Add('')
    $lines.Add("Construction: $(Describe-Amounts $m.authoredConstructionMaterials).")
    foreach($u in $m.unlockRoutes) {
        if($u.type -eq 'corporationLevelReward'){$lines.Add("Unlock reward: $($u.corporation), level $($u.level).")}
        if($u.type -eq 'upgradeStation'){
            foreach($offer in $u.offeredBy){$lines.Add("Upgrade offered by: $($offer.corporation), level $($offer.level).")}
            foreach($task in $u.tasks){$lines.Add("- Task: $($task.text) — $($task.context), $($task.contextValue), count $($task.count).")}
            $lines.Add("Upgrade-station submissions: $(Describe-Amounts $u.researchMaterials).")
        }
    }
    if(-not $m.unlockRoutes.Count){$lines.Add('Unlock route not resolved by the sampled sources; this does not mean initially available.')}
}
$lines.Add('')
$lines.Add('## Remaining boundaries')
$lines.Add('')
$lines.Add('- The selected mode supplies 87 authored building references: 78 normal catalogue candidates and nine Test entries. Every sampled machine belongs to the normal candidate set. Other modes, current unlocks and placement are separate.')
$lines.Add('- Omitted recipe BuildTime resolves to 10 seconds. Crafting defaults are CraftingLoopDuration=0, OverwriteRecipeBuildTime=-1 and bUseCraftingProcessor=true. Explicit values, including zero and false, take precedence. These are not effective production-rate claims.')
$lines.Add('- Corporation level rewards and upgrade tasks are authored definitions, not evidence of current player completion. Collection-based rewards and custom-mode overrides are not merged into a universal unlock rule.')
$lines.Add('- The selected mode/state/keeper chain was read from validated native class defaults. Parent config chains were followed; unrelated parent visualization/replication payloads and Niagara exports remain unsupported. Required config links and selected crafting traits were decoded completely.')
$lines.Add('- Fresh UI observations are recorded separately in game-comparison.json. Coverage applies only to the fields actually visible; completed research hides original submission quantities.')
$lines.Add('')
$lines.Add('See sample-catalogue.json for identities, typed quantities, image references and links back to the private raw export evidence.')
$lines|Set-Content -LiteralPath (Join-Path $root 'sample-catalogue.md') -Encoding utf8NoBOM