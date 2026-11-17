# Finite policy and Git-ancestry fixtures. No network access or production refs are changed.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Release-Policy.ps1')
$cases=0
foreach($case in @(
 @{branch='main';tag='v1.0.0';valid=$true;pre=$false},
 @{branch='update3';tag='v1.1.0-update3.1';valid=$true;pre=$true},
 @{branch='update3';tag='v1.1.0-update3.0';valid=$true;pre=$true},
 @{branch='main';tag='v1.1.0-update3.1';valid=$false},
 @{branch='update3';tag='v1.1.0';valid=$false},
 @{branch='update3';tag='v1.1.0-rc.1';valid=$false},
 @{branch='update3';tag='v1.1.0-update3.01';valid=$false},
 @{branch='main';tag='v01.0.0';valid=$false},
 @{branch='main';tag='V1.0.0';valid=$false},
 @{branch='feature';tag='v1.0.0';valid=$false},
 @{branch='main;exit';tag='v1.0.0';valid=$false},
 @{branch='main';tag='v1.0.0;exit';valid=$false},
 @{branch='main';tag='v1.0.0/extra';valid=$false},
 @{branch='main';tag='v1.0.0+build';valid=$false}
)){
    $policy=$null;$rejected=$false
    try{$policy=Get-ReleasePolicy -Tag $case.tag -Branch $case.branch}catch{$rejected=$true}
    if($rejected -eq $case.valid){throw "Unexpected release policy result: $($case.branch) / $($case.tag)"}
    if($case.valid -and $policy.Prerelease -ne $case.pre){throw 'Wrong prerelease classification.'}
    $cases++
}
function Must-Reject([scriptblock]$Action){$rejected=$false;try{& $Action|Out-Null}catch{$rejected=$true};if(-not $rejected){throw 'Expected release-policy rejection.'}}
Must-Reject {Get-ReleasePolicy -Tag v1.1.0-update3.1 -Branch update3 -Version 1.1.0-update3.2};$cases++
$fixture=Join-Path (Join-Path $PSScriptRoot '../local') ('release-policy-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
Push-Location $fixture
try{
    & git init -q; if($LASTEXITCODE -ne 0){throw 'Fixture initialization failed.'}
    'fixture'|Set-Content fixture.txt
    & git add fixture.txt
    & git -c commit.gpgsign=false -c user.name='Release policy fixture' -c user.email='release-fixture@example.invalid' commit -qm fixture
    if($LASTEXITCODE -ne 0){throw 'Fixture commit failed.'}
    & git branch -M main
    & git update-ref refs/remotes/origin/main HEAD
    & git update-ref refs/remotes/origin/update3 HEAD
    & git -c tag.gpgsign=false tag v1.0.0
    & git -c tag.gpgsign=false tag v1.1.0-update3.1
    $stable=Confirm-ReleaseTag -Tag v1.0.0 -Branch main -Version 1.0.0
    $preview=Confirm-ReleaseTag -Tag v1.1.0-update3.1 -Branch update3 -Version 1.1.0-update3.1
    if($stable.Prerelease -or -not $preview.Prerelease -or $stable.Commit -ne $preview.Commit){throw 'Fixture source/classification mismatch.'};$cases+=2
    Must-Reject {Confirm-ReleaseTag -Tag v9.0.0 -Branch main};$cases++
    'changed'|Set-Content fixture.txt
    & git add fixture.txt
    & git -c commit.gpgsign=false -c user.name='Release policy fixture' -c user.email='release-fixture@example.invalid' commit -qm changed
    & git -c tag.gpgsign=false tag v1.1.0-update3.2
    Must-Reject {Confirm-ReleaseTag -Tag v1.1.0-update3.2 -Branch update3};$cases++
}finally{Pop-Location}
@{status='passed';cases=$cases;networkUsed=$false;productionRefsChanged=$false}|ConvertTo-Json -Compress
