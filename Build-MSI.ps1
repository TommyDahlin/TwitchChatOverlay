<#
.SYNOPSIS
    Builds the Twitch Chat Overlay (release) and packages it as an MSI with WiX Toolset v3.

.DESCRIPTION
    - Reads the package name, version and upgrade GUID from <ProjectRoot>\Cargo.toml
    - Runs "cargo build --release" in the project folder
    - Generates the WiX source itself (no wix\main.wxs needed)
    - Installs per-user to %LOCALAPPDATA%\Programs\TwitchChatOverlay (no admin needed,
      and the app can write settings.ini next to the exe)
    - Bundles every PNG from <ProjectRoot>\img (MOD.png, SUB.png) into an img folder
    - Adds a Start Menu shortcut
    - Writes the MSI to -OutputDir (default: the folder this script lives in)

.EXAMPLE
    .\Build-MSI.ps1
    .\Build-MSI.ps1 -ProjectRoot "D:\Other\ChatReal" -SkipBuild
#>
[CmdletBinding()]
param(
    [string]$ProjectRoot = 'C:\Users\Tommy\Documents\ChatReal',
    [string]$OutputDir   = $PSScriptRoot,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($OutputDir)) { $OutputDir = (Get-Location).Path }

function Write-Step([string]$Text) {
    Write-Host ''
    Write-Host ('=' * 70) -ForegroundColor Cyan
    Write-Host $Text -ForegroundColor Cyan
    Write-Host ('=' * 70) -ForegroundColor Cyan
}

function Esc([string]$s) { [System.Security.SecurityElement]::Escape($s) }

# Runs a native command, streams its output, and throws on a non-zero exit code.
# (cargo writes progress to stderr, which must not be treated as an error.)
function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Exe @Arguments 2>&1 | ForEach-Object { Write-Host $_ }
        $code = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $prev
    }
    if ($code -ne 0) {
        throw ("{0} failed with exit code {1}" -f (Split-Path $Exe -Leaf), $code)
    }
}

# ---------------------------------------------------------------------------
Write-Step 'Validating build environment'
# ---------------------------------------------------------------------------
$ProjectRoot = (Resolve-Path -LiteralPath $ProjectRoot -ErrorAction Stop).Path
$cargoToml   = Join-Path $ProjectRoot 'Cargo.toml'
$mainRs      = Join-Path $ProjectRoot 'src\main.rs'
$imgDir      = Join-Path $ProjectRoot 'img'

if (-not (Test-Path -LiteralPath $cargoToml)) { throw "ERROR: Cargo.toml not found at: $cargoToml" }
if (-not (Test-Path -LiteralPath $mainRs))    { throw "ERROR: src\main.rs not found at: $mainRs" }
Write-Host "[OK] Project root: $ProjectRoot"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'ERROR: cargo was not found on PATH. Install Rust from https://rustup.rs and reopen PowerShell.'
}
Write-Host '[OK] cargo found'

# Locate WiX Toolset v3 (candle.exe / light.exe)
$wixBin = $null
$candidates = @()
if ($env:WIX) { $candidates += (Join-Path $env:WIX 'bin') }
$pf86 = ${env:ProgramFiles(x86)}
if ($pf86) {
    $candidates += @(Get-ChildItem -LiteralPath $pf86 -Directory -Filter 'WiX Toolset v3*' -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending | ForEach-Object { Join-Path $_.FullName 'bin' })
}
foreach ($c in $candidates) {
    if ((Test-Path (Join-Path $c 'candle.exe')) -and (Test-Path (Join-Path $c 'light.exe'))) { $wixBin = $c; break }
}
if (-not $wixBin) {
    throw 'ERROR: WiX Toolset v3 (candle.exe / light.exe) not found. Install it from https://github.com/wixtoolset/wix3/releases'
}
$candle = Join-Path $wixBin 'candle.exe'
$light  = Join-Path $wixBin 'light.exe'
Write-Host "[OK] WiX Toolset found at: $wixBin"

# Read package info from Cargo.toml
$toml = Get-Content -LiteralPath $cargoToml -Raw
if ($toml -match '(?m)^\s*name\s*=\s*"([^"]+)"')                  { $pkgName = $Matches[1] } else { throw 'ERROR: could not read package name from Cargo.toml' }
if ($toml -match '(?m)^\s*version\s*=\s*"(\d+\.\d+\.\d+)')        { $version = $Matches[1] } else { throw 'ERROR: could not read version (x.y.z) from Cargo.toml' }
if ($toml -match 'upgrade-guid\s*=\s*"([0-9A-Fa-f\-]{36})"')      { $upgradeCode = $Matches[1] } else { $upgradeCode = 'B08E2A07-FF9E-4A42-B559-1FE6E1F03E44' }
if ($toml -match '(?m)^\s*authors\s*=\s*\[\s*"([^"]+)"')          { $manufacturer = $Matches[1] } else { $manufacturer = 'Streamer' }
Write-Host "[OK] Package: $pkgName  Version: $version"

# Badge images
$imgFiles = @()
if (Test-Path -LiteralPath $imgDir) {
    $imgFiles = @(Get-ChildItem -LiteralPath $imgDir -Filter '*.png' -File)
}
foreach ($required in @('MOD.png', 'SUB.png')) {
    if (-not ($imgFiles | Where-Object { $_.Name -ieq $required })) {
        Write-Warning "img\$required not found in $imgDir - badges for that type will not show."
    }
}
Write-Host "[OK] $($imgFiles.Count) image(s) will be bundled"

# ---------------------------------------------------------------------------
Write-Step 'Building Rust project (release)'
# ---------------------------------------------------------------------------
if ($SkipBuild) {
    Write-Host 'Skipping cargo build (-SkipBuild)'
}
else {
    Push-Location $ProjectRoot
    try { Invoke-Native 'cargo' @('build', '--release') }
    finally { Pop-Location }
}

$exePath = Join-Path $ProjectRoot "target\release\$pkgName.exe"
if (-not (Test-Path -LiteralPath $exePath)) { throw "ERROR: built exe not found at: $exePath" }
Write-Host "[OK] $exePath"

# ---------------------------------------------------------------------------
Write-Step 'Generating WiX source'
# ---------------------------------------------------------------------------
$work = Join-Path $env:TEMP 'TwitchChatOverlay-msi-build'
if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
New-Item -ItemType Directory -Path $work | Out-Null

$imgComponents = New-Object System.Text.StringBuilder
$imgRefs       = New-Object System.Text.StringBuilder
$i = 0
foreach ($f in $imgFiles) {
    $i++
    [void]$imgComponents.AppendLine("      <Component Id=`"ImgComp$i`" Guid=`"*`">")
    [void]$imgComponents.AppendLine("        <File Id=`"ImgFile$i`" Source=`"$(Esc $f.FullName)`" KeyPath=`"yes`" />")
    [void]$imgComponents.AppendLine('      </Component>')
    [void]$imgRefs.AppendLine("      <ComponentRef Id=`"ImgComp$i`" />")
}

$wxs = @"
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="*"
           Name="Twitch Chat Overlay"
           Language="1033"
           Version="$version"
           Manufacturer="$(Esc $manufacturer)"
           UpgradeCode="$upgradeCode">

    <Package InstallerVersion="500" Compressed="yes" InstallScope="perUser"
             Description="Twitch chat overlay with always-on-top window and MOD/SUB badges" />

    <MajorUpgrade AllowSameVersionUpgrades="yes"
                  DowngradeErrorMessage="A newer version of Twitch Chat Overlay is already installed." />
    <MediaTemplate EmbedCab="yes" />

    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="LocalAppDataFolder">
        <Directory Id="ProgramsDir" Name="Programs">
          <Directory Id="INSTALLFOLDER" Name="TwitchChatOverlay">
            <Directory Id="ImgDir" Name="img" />
          </Directory>
        </Directory>
      </Directory>
      <Directory Id="ProgramMenuFolder">
        <Directory Id="StartMenuDir" Name="Twitch Chat Overlay" />
      </Directory>
    </Directory>

    <DirectoryRef Id="INSTALLFOLDER">
      <Component Id="MainExeComp" Guid="*">
        <File Id="MainExeFile" Source="$(Esc $exePath)" KeyPath="yes" />
        <!-- settings.ini is created by the app at runtime; remove it on uninstall -->
        <RemoveFile Id="RemoveSettingsIni" Name="settings.ini" On="uninstall" />
      </Component>
    </DirectoryRef>

    <DirectoryRef Id="ImgDir">
$($imgComponents.ToString())    </DirectoryRef>

    <DirectoryRef Id="StartMenuDir">
      <Component Id="ShortcutComp" Guid="*">
        <Shortcut Id="AppShortcut"
                  Name="Twitch Chat Overlay"
                  Target="[#MainExeFile]"
                  WorkingDirectory="INSTALLFOLDER" />
        <RemoveFolder Id="RemoveStartMenuDir" On="uninstall" />
        <RegistryValue Root="HKCU" Key="Software\TwitchChatOverlay" Name="installed" Type="integer" Value="1" KeyPath="yes" />
      </Component>
    </DirectoryRef>

    <Feature Id="MainFeature" Title="Twitch Chat Overlay" Level="1">
      <ComponentRef Id="MainExeComp" />
$($imgRefs.ToString())      <ComponentRef Id="ShortcutComp" />
    </Feature>
  </Product>
</Wix>
"@

$wxsPath     = Join-Path $work 'Product.wxs'
$wixobjPath  = Join-Path $work 'Product.wixobj'
Set-Content -LiteralPath $wxsPath -Value $wxs -Encoding UTF8
Write-Host "[OK] $wxsPath"

# ---------------------------------------------------------------------------
Write-Step 'Compiling and linking MSI'
# ---------------------------------------------------------------------------
if (-not (Test-Path -LiteralPath $OutputDir)) { New-Item -ItemType Directory -Path $OutputDir | Out-Null }
$msiPath = Join-Path $OutputDir "$pkgName-$version.msi"
if (Test-Path -LiteralPath $msiPath) { Remove-Item -LiteralPath $msiPath -Force }

Invoke-Native $candle @('-nologo', '-arch', 'x64', '-out', $wixobjPath, $wxsPath)

# ICE38/64/91 only complain about per-user installs; they are expected here.
Invoke-Native $light @('-nologo', '-spdb', '-sice:ICE38', '-sice:ICE64', '-sice:ICE91', '-out', $msiPath, $wixobjPath)

if (-not (Test-Path -LiteralPath $msiPath)) { throw 'ERROR: light.exe finished but the MSI was not created.' }
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue

# ---------------------------------------------------------------------------
Write-Step 'Done'
# ---------------------------------------------------------------------------
$size = [math]::Round((Get-Item -LiteralPath $msiPath).Length / 1MB, 2)
Write-Host "[OK] MSI created: $msiPath ($size MB)" -ForegroundColor Green
Write-Host "     Installs to: %LOCALAPPDATA%\Programs\TwitchChatOverlay"
