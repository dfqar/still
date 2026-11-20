#Requires -Version 7.0
[CmdletBinding()]
param([switch]$SkipChecks)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') {
    throw 'Use an x64 Windows PowerShell 7 process with the MSVC Rust toolchain.'
}
$projectRoot = Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot
try {
    if (-not $SkipChecks) {
        & cargo xtask check-rust
        if ($LASTEXITCODE -ne 0) { throw 'Rust checks failed.' }
    }
    & cargo xtask windows
    if ($LASTEXITCODE -ne 0) { throw 'Windows engine build failed.' }
    & "$PSScriptRoot/test-windows-abi.ps1" -LibraryPath "$projectRoot/build/windows/win-x64/still_ffi.dll"
    $bundle = Join-Path $projectRoot 'build/windows/win-x64'
    Get-ChildItem $bundle -File | Where-Object Name -ne 'SHA256SUMS.txt' |
        Sort-Object Name | ForEach-Object {
            $hash = Get-FileHash $_.FullName -Algorithm SHA256
            '{0}  {1}' -f $hash.Hash.ToLowerInvariant(), $_.Name
        } | Set-Content (Join-Path $bundle 'SHA256SUMS.txt') -Encoding utf8NoBOM
    Write-Host "Engine bundle ready: $bundle (SwapChainPanel integration remains W0)."
} finally {
    Pop-Location
}
