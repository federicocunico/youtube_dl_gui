[CmdletBinding()]
param([ValidateSet('all','setup','check','test','build','package','smoke','live')][string]$Task = 'all')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Set-Location (Split-Path -Parent $PSScriptRoot)

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}
function Assert-Prerequisites {
    foreach ($program in @('node.exe','npm.cmd','cargo.exe','rustup.exe','tar.exe')) {
        if (-not (Get-Command $program -ErrorAction SilentlyContinue)) {
            throw "Missing $program. See docs/DEVELOPMENT.md for Windows prerequisites."
        }
    }
    $requiredNode = (Get-Content .node-version -Raw).Trim()
    $installedNode = (& node.exe --version).TrimStart('v')
    if ($installedNode -ne $requiredNode) { throw "Use Node $requiredNode (found $installedNode)." }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Install Visual Studio Build Tools with Desktop development with C++ and a Windows SDK.' }
    $compiler = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $compiler) { throw 'The Visual C++ x64 compiler is missing. Install Desktop development with C++.' }
}
function Setup-Environment {
    Assert-Prerequisites
    $toolchain = [regex]::Match((Get-Content rust-toolchain.toml -Raw), 'channel\s*=\s*"([^"]+)"').Groups[1].Value
    if (-not $toolchain) { throw 'Missing pinned Rust toolchain.' }
    Invoke-Checked 'rustup.exe' @('toolchain','install',$toolchain,'--profile','minimal','--component','rustfmt','--no-self-update')
    Invoke-Checked 'npm.cmd' @('ci')
    Invoke-Checked 'npm.cmd' @('run','runtime')
    Invoke-Checked 'npx.cmd' @('playwright','install','chromium')
}
function Build-Installer {
    Invoke-Checked 'npm.cmd' @('run','tauri','--','build','--ci','--bundles','nsis')
}
function Test-Installed {
    $version = (Get-Content package.json -Raw | ConvertFrom-Json).version
    $installers = @(Get-ChildItem -LiteralPath src-tauri/target/release/bundle/nsis -Filter "YT Saver_$($version)_x64-setup.exe")
    if ($installers.Count -ne 1) { throw 'Run make build first: expected exactly one NSIS installer.' }
    Invoke-Checked 'node.exe' @('scripts/smoke-install.mjs',$installers[0].FullName)
}
try {
    switch ($Task) {
        'setup' { Setup-Environment }
        'check' { Invoke-Checked 'npm.cmd' @('run','check'); Invoke-Checked 'node.exe' @('scripts/check-runtime.mjs') }
        'test' { Invoke-Checked 'npm.cmd' @('run','verify') }
        'build' { Build-Installer }
        'package' { Build-Installer; Invoke-Checked 'node.exe' @('scripts/collect-artifacts.mjs') }
        'smoke' { Test-Installed }
        'live' { Invoke-Checked 'npm.cmd' @('run','verify:live') }
        'all' {
            Setup-Environment
            Invoke-Checked 'npm.cmd' @('run','verify')
            Build-Installer
            Test-Installed
            Invoke-Checked 'node.exe' @('scripts/collect-artifacts.mjs')
        }
    }
    Write-Host "YT Saver: $Task completed successfully."
} catch { Write-Error $_; exit 1 }
