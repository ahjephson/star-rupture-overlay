param([Parameter(Mandatory)][string]$ExecutablePath, [Parameter(Mandatory)][string]$OutputDirectory)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$exe=(Resolve-Path -LiteralPath $ExecutablePath).Path
$work=[IO.Path]::GetFullPath($OutputDirectory)
if(Test-Path -LiteralPath $work){throw 'Choose a fresh lifecycle output directory.'}
if(@(Get-Process -Name @('starrupture-overlay','starrupture-wave-overlay') -ErrorAction SilentlyContinue).Count){throw 'Exit the running overlay before this optional lifecycle test.'}
New-Item -ItemType Directory -Path $work|Out-Null
Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class OverlayLifecycleWindow {
    public delegate bool Visit(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumWindows(Visit visit, IntPtr data);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr window, StringBuilder name, int length);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr window,uint message,UIntPtr w,IntPtr l);
    public static IntPtr Find(uint pid) {
        IntPtr result=IntPtr.Zero;
        EnumWindows((window,data)=> {uint owner;GetWindowThreadProcessId(window,out owner);var name=new StringBuilder(256);GetClassNameW(window,name,256);if(owner==pid && name.ToString()=="StarRuptureWaveTimer"){result=window;return false;}return true;},IntPtr.Zero);
        return result;
    }
}
"@
function StartFixture([string]$Name,[string[]]$Arguments=@()){
    $data=Join-Path $work $Name; New-Item -ItemType Directory -Force -Path (Join-Path $data 'StarRuptureWaveOverlay')|Out-Null
    # Extraction has its own cold/offline diagnostics; lifecycle fixtures disable it explicitly.
    '{"recipeTracker":{"enabled":false}}'|Set-Content (Join-Path $data 'StarRuptureWaveOverlay/preferences.json') -Encoding utf8NoBOM
    $parameters=@{FilePath=$exe;WorkingDirectory=$work;WindowStyle='Hidden';PassThru=$true;Environment=@{LOCALAPPDATA=$data};RedirectStandardOutput=(Join-Path $work ($Name+'-stdout.log'));RedirectStandardError=(Join-Path $work ($Name+'-stderr.log'))}
    if($Arguments.Count){$parameters.ArgumentList=@($Arguments|ForEach-Object {'"'+$_+'"'})}
    Start-Process @parameters
}
function WaitFor([scriptblock]$Predicate,[Diagnostics.Process]$Process,[int]$Seconds){
    $deadline=[DateTime]::UtcNow.AddSeconds($Seconds)
    while([DateTime]::UtcNow -lt $deadline){$value=& $Predicate;if($value){return $value};if($Process.HasExited){throw 'Lifecycle app exited early.'};Start-Sleep -Milliseconds 25}
    throw 'Lifecycle deadline exceeded.'
}
$reader=StartFixture 'reader' @('--self-test-reader',(Join-Path $work 'reader-validation'))
try{if(-not $reader.WaitForExit(70000)){$reader.Kill($true);throw 'Live reader deadline exceeded.'};if($reader.ExitCode -ne 0){throw 'Live reader diagnostic failed.'}}finally{$reader.Dispose()}
$cases=@()
foreach($mode in @('normal','cold-cancel')){
    $process=StartFixture $mode
    try{
        # PowerShell treats IntPtr.Zero as true; return only an actual owned HWND.
        $window=WaitFor {$found=[OverlayLifecycleWindow]::Find([uint32]$process.Id);if($found -ne [IntPtr]::Zero){$found}} $process 10
        $log=Join-Path $work ($mode+'-stderr.log')
        if($mode -eq 'normal'){
            $null=WaitFor {(Get-Content -LiteralPath $log -Raw -ErrorAction SilentlyContinue) -match 'first validated live observation'} $process 65
            $duplicate=StartFixture 'duplicate'
            try{if(-not $duplicate.WaitForExit(5000) -or $duplicate.ExitCode -ne 0){throw 'Single-instance check failed.'}}finally{if(-not $duplicate.HasExited){$duplicate.Kill($true)};$duplicate.Dispose()}
        }else{$null=WaitFor {(Get-Content -LiteralPath $log -Raw -ErrorAction SilentlyContinue) -match 'Discovering current Unreal runtime references'} $process 60}
        $timer=[Diagnostics.Stopwatch]::StartNew()
        if(-not [OverlayLifecycleWindow]::PostMessageW($window,0x10,[UIntPtr]::Zero,[IntPtr]::Zero)){throw 'Could not close owned fixture window.'}
        if(-not $process.WaitForExit(10000) -or $process.ExitCode -ne 0){throw 'Owned app did not stop cleanly.'}
        if((Get-Content -LiteralPath $log -Raw) -notmatch 'no reads remain in flight'){throw 'Reader shutdown not confirmed.'}
        $cases+=@{mode=$mode;shutdownMs=$timer.ElapsedMilliseconds;isolatedPreferences=$true;recipeSetupDisabled=$true}
    }finally{if(-not $process.HasExited){$window=[OverlayLifecycleWindow]::Find([uint32]$process.Id);if($window -ne [IntPtr]::Zero){$null=[OverlayLifecycleWindow]::PostMessageW($window,0x10,[UIntPtr]::Zero,[IntPtr]::Zero)};if(-not $process.WaitForExit(10000)){$process.Kill($true)}};$process.Dispose()}
}
@{status='passed';singleInstance='passed';cases=$cases}|ConvertTo-Json -Depth 5|Set-Content (Join-Path $work 'validation.json') -Encoding utf8NoBOM
Get-Content (Join-Path $work 'validation.json')
