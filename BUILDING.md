# Building the MSI Installer

This directory contains automated scripts to build the Twitch Chat Overlay MSI installer using WiX Toolset.

## Prerequisites

1. **Rust Toolchain** - Install from https://www.rust-lang.org/
2. **WiX Toolset v3.14** - Download and install from https://wixtoolset.org/

## Quick Start

### Option 1: Double-Click (Easiest)
Simply double-click `Build-MSI.bat` and the installer will be built automatically.

### Option 2: PowerShell (More Control)
Open PowerShell in this directory and run:
```powershell
.\Build-MSI.ps1
```

### Option 3: PowerShell with Verbose Output
```powershell
.\Build-MSI.ps1 -Verbose
```

### Option 4: Skip Rust Build (Faster Rebuilds)
If you've already built the release binary:
```powershell
.\Build-MSI.ps1 -SkipRustBuild
```

## Output

The final MSI installer will be created at:
```
src\target\wix\TwitchChatOverlay-1.0.0-x86_64.msi
```

## Build Process

The scripts automate the following steps:

1. **Validate Environment**
   - Checks that WiX Toolset is installed
   - Verifies all required files exist

2. **Build Rust Project** (unless `-SkipRustBuild` is used)
   - Runs `cargo build --release`
   - Compiles the application binary

3. **Compile WiX Source** (candle.exe)
   - Compiles `wix/main.wxs` to `target/wix/main.wixobj`
   - Passes version and binary path variables

4. **Link MSI** (light.exe)
   - Links the object file with WiX UI extensions
   - Produces the final `TwitchChatOverlay-1.0.0-x86_64.msi`

5. **Display Summary**
   - Shows file size, location, and completion status

## Troubleshooting

### "candle.exe not found"
- Verify WiX Toolset v3.14 is installed at `C:\Program Files (x86)\WiX Toolset v3.14`
- If installed elsewhere, edit the script and update `$WixToolsetPath`

### "Cargo build failed"
- Run `cd src && cargo build --release` directly to see detailed error
- Ensure Rust toolchain is up to date: `rustup update`

### "main.wxs not found"
- Ensure you're running the script from the ChatReal project root directory
- File should be at `src/wix/main.wxs`

## Customization

To customize the build script:
- Open `Build-MSI.ps1` in a text editor
- Edit the parameters at the top:
  - `$AppName` - Application name
  - `$AppVersion` - Application version
  - `$WixToolsetPath` - Path to WiX Toolset installation

## Distribution

Once the MSI is built, you can:
1. Share it directly with users
2. Upload to GitHub Releases
3. Host on your website
4. Include in an auto-updater

Users can install by:
- Double-clicking the MSI
- Running `msiexec /i TwitchChatOverlay-1.0.0-x86_64.msi`
- Via Group Policy (for enterprise deployments)
