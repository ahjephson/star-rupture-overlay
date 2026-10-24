$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
if(Get-Process starrupture-wave-overlay -ErrorAction SilentlyContinue){throw 'An overlay is already running. Exit it from the tray before starting another.'}
$exe=Join-Path $root 'dist/starrupture-wave-overlay.exe'
$cfg=Join-Path $root 'local/overlay.json'
if(!(Test-Path -LiteralPath $exe) -or !(Test-Path -LiteralPath $cfg)){throw 'Run scripts/Build.ps1 first'}
$p=Start-Process -FilePath $exe -ArgumentList @('--config',('"'+$cfg+'"')) -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $root 'local/overlay-stderr.txt') -RedirectStandardOutput (Join-Path $root 'local/overlay-stdout.txt')
Write-Output ('Overlay PID '+$p.Id)
