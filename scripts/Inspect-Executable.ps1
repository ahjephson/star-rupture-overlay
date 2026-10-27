param([Parameter(Mandatory)][string]$Path)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$bytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Path).Path)
function U16([int]$offset) { if ($offset -lt 0 -or $offset + 2 -gt $bytes.Length) { throw 'Truncated executable.' }; [BitConverter]::ToUInt16($bytes, $offset) }
function U32([int]$offset) { if ($offset -lt 0 -or $offset + 4 -gt $bytes.Length) { throw 'Truncated executable.' }; [BitConverter]::ToUInt32($bytes, $offset) }
if ($bytes.Length -lt 64 -or $bytes[0] -ne 77 -or $bytes[1] -ne 90) { throw 'Invalid DOS header.' }
$pe = [int](U32 60)
if ((U32 $pe) -ne 0x4550 -or (U16 ($pe + 4)) -ne 0x8664) { throw 'Release must be Windows x64 PE.' }
$sections = U16 ($pe + 6)
$optional = $pe + 24
if ((U16 $optional) -ne 0x20b -or (U16 ($optional + 68)) -ne 2) { throw 'Release must use PE32+ GUI subsystem.' }
$headers = $optional + (U16 ($pe + 20))
function FileOffset([uint32]$rva) {
    for ($i = 0; $i -lt $sections; $i++) {
        $section = $headers + $i * 40
        $virtualSize = U32 ($section + 8); $address = U32 ($section + 12)
        $rawSize = U32 ($section + 16); $raw = U32 ($section + 20)
        if ($rva -ge $address -and [uint64]$rva -lt [uint64]$address + [Math]::Max($virtualSize, $rawSize)) {
            $offset = [uint64]$raw + $rva - $address
            if ($offset -ge $bytes.Length) { throw 'Invalid section address.' }
            return [int]$offset
        }
    }
    throw 'Unmapped executable address.'
}
$imports = [Collections.Generic.List[string]]::new()
$importRva = U32 ($optional + 120)
if ($importRva) {
    $descriptor = FileOffset $importRva
    for ($i = 0; ; $i++) {
        if ($i -ge 4096) { throw 'Import descriptor limit exceeded.' }
        $values = @(0..4 | ForEach-Object { U32 ($descriptor + $_ * 4) })
        if (-not ($values | Where-Object { $_ -ne 0 })) { break }
        $start = FileOffset $values[3]
        $end = $start
        while ($end -lt $bytes.Length -and $bytes[$end] -ne 0 -and $end - $start -le 512) { $end++ }
        if ($end -ge $bytes.Length -or $end - $start -gt 512) { throw 'Invalid import name.' }
        $imports.Add([Text.Encoding]::ASCII.GetString($bytes, $start, $end - $start))
        $descriptor += 20
    }
}
if ($imports | Where-Object { $_ -match '^(vcruntime|msvcp|msvcr)' }) { throw 'VC runtime redistributable dependency.' }
$ascii = [Text.Encoding]::ASCII.GetString($bytes)
foreach ($marker in @('C:' + [char]92 + 'Users' + [char]92, ('C:' + '/Users/'))) {
    if ($ascii.Contains($marker, [StringComparison]::OrdinalIgnoreCase)) { throw 'Personal build path embedded.' }
}
@{target='windows-x64';subsystem='GUI';bytes=$bytes.Length;imports=@($imports);separateVcRuntimeRequired=$false} | ConvertTo-Json -Depth 4
