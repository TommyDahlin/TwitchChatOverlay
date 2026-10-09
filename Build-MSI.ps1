<#
.SYNOPSIS
Automated MSI builder for Twitch Chat Overlay using WiX Toolset.

.DESCRIPTION
This script performs a complete build pipeline:
1. Builds the Rust project in release mode
2. Compiles the WiX source file (main.wxs) to an object file
3. Links the object file into an MSI installer
4. Outputs the final MSI to target/wix/

.EXAMPLE
.\Build-MSI.ps1
# Builds with default settings

.\Build-MSI.ps1 -Verbose
# Shows detailed output

.\Build-MSI.ps1 -SkipRustBuild
# Skips cargo build and uses existing release binary
#>

param(
    [string]$ProjectRoot = $PSScriptRoot,
    [string]$AppName = "TwitchChatOverlay",
    [string]$AppVersion = "1.0.0",
    [string]$WixToolsetPath = "C:\Program Files (x86)\WiX Toolset v3.14\bin",
    [switch]$SkipRustBuild,
    [switch]$Verbose
)

# Enable strict error handling
$ErrorActionPreference = "Stop"
$VerbosePreference = if ($Verbose) { "Continue" } else { "SilentlyContinue" }

# Helper functions
function Write-Header {
    param([string]$Message)
    Write-Host "`n" -NoNewline
    Write-Host ("=" * 70) -ForegroundColor Cyan
    Write-Host $Message -ForegroundColor Cyan
    Write-Host ("=" * 70) -ForegroundColor Cyan
}

function Write-Success {
    param([string]$Message)
    Write-Host "✓ $Message" -ForegroundColor Green
}

function Write-Error-Custom {
    param([string]$Message)
    Write-Host "✗ $Message" -ForegroundColor Red
}

function Test-Path-Exists {
    param([string]$Path, [string]$Description)
    if (-not (Test-Path $Path)) {
        throw "ERROR: $Description not found at: $Path"
    }
    Write-Verbose "Found $Description at: $Path"
}

# ==================== VALIDATION ====================
Write-Header "Validating Build Environment"

# Check for required tools
$candle = Join-Path $WixToolsetPath "candle.exe"
$light = Join-Path $WixToolsetPath "light.exe"

Test-Path-Exists $candle "candle.exe"
Test-Path-Exists $light "light.exe"

# Check for WixUIExtension
$wixUIExt = Join-Path $WixToolsetPath "WixUIExtension.dll"
Test-Path-Exists $wixUIExt "WixUIExtension.dll"

# Check project structure
$cargoToml = Join-Path $ProjectRoot "src" "Cargo.toml"
$mainWxs = Join-Path $ProjectRoot "src" "wix" "main.wxs"
Test-Path-Exists $cargoToml "Cargo.toml"
Test-Path-Exists $mainWxs "main.wxs"

Write-Success "WiX Toolset found at: $WixToolsetPath"
Write-Success "Project structure is valid"

# ==================== RUST BUILD ====================
if ($SkipRustBuild) {
    Write-Host "`n[SKIPPED] Rust release build" -ForegroundColor Yellow
} else {
    Write-Header "Building Rust Project (Release)"
    
    Push-Location (Join-Path $ProjectRoot "src")
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build failed with exit code $LASTEXITCODE"
        }
        Write-Success "Rust project built successfully"
    }
    finally {
        Pop-Location
    }
}

# ==================== CANDLE COMPILATION ====================
Write-Header "Compiling WiX Source (candle.exe)"

$binDir = Join-Path $ProjectRoot "src" "target" "release"
$outputDir = Join-Path $ProjectRoot "src" "target" "wix"
$wxsFile = Join-Path $ProjectRoot "src" "wix" "main.wxs"
$wixobjFile = Join-Path $outputDir "main.wixobj"

# Create output directory if it doesn't exist
if (-not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir | Out-Null
    Write-Verbose "Created output directory: $outputDir"
}

Write-Verbose "Running: $candle"
Write-Verbose "  -arch x64"
Write-Verbose "  -o $outputDir"
Write-Verbose "  -dCargoTargetBinDir=$binDir"
Write-Verbose "  -dVersion=$AppVersion"
Write-Verbose "  $wxsFile"

& $candle -arch x64 -o "$outputDir\" `
    "-dCargoTargetBinDir=$binDir" `
    "-dVersion=$AppVersion" `
    $wxsFile

if ($LASTEXITCODE -ne 0) {
    throw "Candle compilation failed with exit code $LASTEXITCODE"
}

Write-Success "WiX source compiled to: $wixobjFile"

# ==================== ENSURE LICENSE FILE ====================
# Ensure License.rtf exists in the expected location (relative to working dir)
$licenseFile = Join-Path $ProjectRoot "wix\License.rtf"
$srcLicenseFile = Join-Path $ProjectRoot "src\wix\License.rtf"

if (-not (Test-Path $licenseFile)) {
    if (Test-Path $srcLicenseFile) {
        Write-Verbose "License.rtf not in expected location, copying from src\wix\"
        $licenseDir = Split-Path $licenseFile -Parent
        if (-not (Test-Path $licenseDir)) {
            New-Item -ItemType Directory -Path $licenseDir | Out-Null
        }
        Copy-Item -Path $srcLicenseFile -Destination $licenseFile
        Write-Verbose "Copied License.rtf to: $licenseFile"
    }
    else {
        throw "License.rtf not found at $srcLicenseFile"
    }
}

# ==================== LIGHT LINKING ====================
Write-Header "Linking MSI (light.exe)"

$msiFile = Join-Path $outputDir "$AppName-$AppVersion-x86_64.msi"

Write-Verbose "Running: $light"
Write-Verbose "  -out $msiFile"
Write-Verbose "  -ext $wixUIExt"
Write-Verbose "  $wixobjFile"

& $light -out $msiFile `
    -ext "$wixUIExt" `
    $wixobjFile

if ($LASTEXITCODE -ne 0) {
    throw "Light linking failed with exit code $LASTEXITCODE"
}

Write-Success "MSI installer created: $msiFile"

# ==================== VERIFICATION ====================
Write-Header "Build Summary"

$msiInfo = Get-Item $msiFile
Write-Host "File:     $($msiInfo.Name)" -ForegroundColor White
Write-Host "Size:     $([Math]::Round($msiInfo.Length / 1MB, 2)) MB" -ForegroundColor White
Write-Host "Created:  $($msiInfo.LastWriteTime)" -ForegroundColor White
Write-Host "Path:     $($msiInfo.FullName)" -ForegroundColor White

Write-Host "`n" -NoNewline
Write-Host ("=" * 70) -ForegroundColor Green
Write-Host "✓ BUILD SUCCESSFUL!" -ForegroundColor Green
Write-Host ("=" * 70) -ForegroundColor Green

Write-Host "`nYour installer is ready for distribution!`n" -ForegroundColor Green
