$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$exe=Join-Path $root 'dist/starrupture-overlay.exe'
if(!(Test-Path -LiteralPath $exe)){throw 'Run scripts/Build.ps1 first'}
# The application owns defaults, single-instance protection and its read-only worker.
$p=Start-Process -FilePath $exe -WindowStyle Hidden -PassThru
Write-Output ('Overlay PID '+$p.Id)
