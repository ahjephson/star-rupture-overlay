# Branch/tag policy is shared by draft jobs and tested without GitHub credentials.
function Get-ReleasePolicy {
    param([Parameter(Mandatory)][string]$Tag,[Parameter(Mandatory)][string]$Branch,[string]$Version)
    $number='(?:0|[1-9][0-9]*)'
    $base="v$number\.$number\.$number"
    if ($Branch -cnotin @('main','update3')) { throw 'Release drafts must be dispatched from main or update3.' }
    $prerelease=$Branch -ceq 'update3'
    $pattern=if($prerelease){"^$base-update3\.$number`$"}else{"^$base`$"}
    if($Tag -cnotmatch $pattern){throw 'main requires a stable version tag; update3 requires vMAJOR.MINOR.PATCH-update3.N.'}
    if($Version -and $Tag -cne "v$Version"){throw 'Tag and app version differ.'}
    [pscustomobject]@{Tag=$Tag;Branch=$Branch;Prerelease=$prerelease;TagRef="refs/tags/$Tag";Version=$Tag.Substring(1)}
}
function Confirm-ReleaseTag {
    param([Parameter(Mandatory)][string]$Tag,[Parameter(Mandatory)][string]$Branch,[string]$Version)
    $policy=Get-ReleasePolicy -Tag $Tag -Branch $Branch -Version $Version
    $commit=& git rev-parse --verify "$($policy.TagRef)^{commit}" 2>$null
    if($LASTEXITCODE -ne 0){throw 'Tag must already exist; draft preparation never creates tags.'}
    & git merge-base --is-ancestor $commit "refs/remotes/origin/$($policy.Branch)"
    if($LASTEXITCODE -ne 0){throw 'Release tag must be reachable from its selected release branch.'}
    $policy|Add-Member -NotePropertyName Commit -NotePropertyValue $commit
    $policy
}
