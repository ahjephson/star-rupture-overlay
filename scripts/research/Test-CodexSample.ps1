param([Parameter(Mandatory)][string]$PrivateRoot)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'New-CodexSample.ps1') -PrivateRoot $PrivateRoot
function Assert($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
$sample = Get-Content (Join-Path $root 'sample-catalogue.json') -Raw | ConvertFrom-Json -AsHashtable -Depth 100
$comparisons = 0
foreach ($recipe in $sample.recipes) {
    $raw = $records[$recipe.id].exports[0].properties
    Assert ($recipe.output.itemId -ceq $raw.OutputItem.Item.Split('.')[0] -and $recipe.output.quantity -eq $raw.OutputItem.Count) 'Recipe output differs from source'
    Assert ($recipe.inputs.Count -eq $raw.NeededResources.Count) 'Recipe input count differs from source'
    for ($i=0; $i -lt $recipe.inputs.Count; $i++) {
        Assert ($recipe.inputs[$i].itemId -ceq $raw.NeededResources[$i].Item.Split('.')[0] -and $recipe.inputs[$i].quantity -eq $raw.NeededResources[$i].Count) 'Recipe input differs from source'
    }
    Assert ($recipe.unlock.researchMaterials.Count -eq $(if ($null -eq $raw.UnlockRequirements) { 0 } else { $raw.UnlockRequirements.Count })) 'Research bill count differs from source'
    for ($i=0; $i -lt $recipe.unlock.researchMaterials.Count; $i++) {
        Assert ($recipe.unlock.researchMaterials[$i].itemId -ceq $raw.UnlockRequirements[$i].Key.Split('.')[0] -and $recipe.unlock.researchMaterials[$i].quantity -eq $raw.UnlockRequirements[$i].Value) 'Research bill differs from source'
    }
    Assert ($recipe.baseDurationSeconds.value -eq $raw.BuildTime) 'Explicit recipe time was replaced'
    foreach ($producer in $recipe.producerIds) {
        $machine = @($sample.machines | Where-Object { $_.id -ceq $producer })
        Assert ($machine.Count -eq 1 -and $recipe.id -cin $machine[0].recipes -and $machine[0].modeEligibility.normalCatalogueCandidate) 'Producer link is outside selected-mode candidates'
    }
    $comparisons++
}
Assert ((Default-Value @{BuildTime=0} 'BuildTime' 'CrItemRecipeData').value -eq 0) 'Explicit zero must survive default resolution'
Assert ((Default-Value @{AddBasePlatformCost=$false} 'AddBasePlatformCost' 'CrBuildingData').origin -eq 'serialized asset') 'Explicit false must survive default resolution'
Assert ((Default-Value @{AddBasePlatformCost=$true} 'AddBasePlatformCost' 'CrBuildingData').value -eq $true) 'Explicit true must override native false'
Assert ((Default-Value @{} 'BuildTime' 'CrItemRecipeData').value -eq 10) 'Missing recipe duration must use matching native CDO'
$rejected = $false
try { Default-Value @{} 'UnprovenField' 'CrItemRecipeData' | Out-Null } catch { $rejected = $true }
Assert $rejected 'An unproven default must be rejected'
$synthetic = '/Game/Chimera/Research/Synthetic'
$records[$synthetic] = @{exports=@(
    @{name='Selected';schema='CrBuildingCraftingTrait';completeObjectTail=$true;properties=@{CraftingParameters=@{RecipeCollection='selected'}}},
    @{name='Unused';schema='CrBuildingCraftingTrait';completeObjectTail=$true;properties=@{CraftingParameters=@{RecipeCollection='wrong'}}}
)}
$selected = Selected-CraftingTrait @(@{asset=$synthetic;config=@{Traits=@('Synthetic.Selected')}})
Assert ($selected.name -ceq 'Selected') 'An unselected export must not participate in trait projection'
$records.Remove($synthetic) | Out-Null
Assert ($sample.selectedMode.authoredBuildingCount -eq 87 -and $sample.selectedMode.normalCandidateCount -eq 78 -and $sample.selectedMode.excludedTestCount -eq 9) 'Mode root/category evidence changed; review before extending coverage'
$unrooted = @($modeBuildings.Keys | Where-Object { $_ -match '/SmelterTier2/' })
Assert ($unrooted.Count -eq 0) 'Smelter v.2 negative control changed; review mode eligibility'
[ordered]@{status='passed';recipeSourceComparisons=$comparisons;sampleMachines=$sample.machines.Count;defaultAndSelectionRegressions=6;authoredBuildings=87;normalCatalogueCandidates=78;testEntriesExcluded=9}|ConvertTo-Json|Set-Content (Join-Path $root 'mode-defaults-validation.json') -Encoding utf8NoBOM
Get-Content (Join-Path $root 'mode-defaults-validation.json')