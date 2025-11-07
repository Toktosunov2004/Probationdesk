# Flutter Build Fix Guide

## Problem

If you encounter this error during Flutter build:

```
error C1083: Cannot open include file:
'include/flutter_gpu_texture_renderer/flutter_gpu_texture_renderer_plugin_c_api.h':
No such file or directory
```

This indicates a Flutter plugin dependency issue.

## Quick Fix

### Option 1: Automated Fix (Recommended)

Run the automated fix script from the project root:

```powershell
.\fix_flutter_build.ps1
```

This script will:
1. Clean Flutter build cache
2. Remove problematic plugin caches
3. Delete pubspec.lock
4. Fetch fresh dependencies
5. Rebuild the Windows application

### Option 2: Manual Fix

```powershell
cd work\probationdesk_src\flutter

# Clean the build
flutter clean

# Remove lock file
Remove-Item pubspec.lock -Force

# Clear plugin cache
Remove-Item -Path "$env:LOCALAPPDATA\Pub\Cache\git\flutter_gpu_texture_renderer*" -Recurse -Force -ErrorAction SilentlyContinue

# Re-fetch dependencies
flutter pub get
flutter pub upgrade

# Rebuild
flutter build windows --release
```

### Option 3: Diagnose First

If you want to understand what's wrong before fixing:

```powershell
.\diagnose_flutter.ps1
```

This will check:
- Flutter and Dart versions
- Pub cache location and contents
- Git plugin installations
- Plugin symlinks
- Missing files and directories

## Scripts Included

### `fix_flutter_build.ps1`
Automated script that cleans and rebuilds the Flutter application, fixing common plugin issues.

**Usage:**
```powershell
.\fix_flutter_build.ps1
```

**What it does:**
- Cleans Flutter build cache
- Removes problematic Git plugin caches
- Deletes pubspec.lock to force fresh resolution
- Fetches and upgrades dependencies
- Verifies plugin symlinks
- Builds the Windows application
- Reports build status and executable location

### `diagnose_flutter.ps1`
Diagnostic script that checks your Flutter environment and identifies issues.

**Usage:**
```powershell
.\diagnose_flutter.ps1
```

**What it checks:**
- Flutter SDK version
- Dart SDK version
- Pub cache location and size
- Git plugins in cache (especially flutter_gpu_texture_renderer)
- Plugin symlinks status
- Header and source file existence
- Build directory status
- Provides specific recommendations

## Why This Error Occurs

This error typically happens when:

1. **Plugin not properly fetched**: The Git-based Flutter plugin wasn't downloaded correctly
2. **Stale cache**: Old plugin files are cached and Flutter isn't refreshing them
3. **Incomplete plugin**: The plugin repository at the specified commit might be missing files
4. **Symlink issues**: Windows symlinks for plugins weren't created properly

## Prevention

To avoid this issue in the future:

1. Always run `flutter pub get` after pulling code changes
2. If you see plugin-related warnings, don't ignore them
3. Periodically clean your pub cache: `flutter pub cache repair`
4. Keep Flutter SDK updated: `flutter upgrade`

## Additional Help

If the scripts don't solve your issue:

1. Check Flutter doctor:
   ```powershell
   flutter doctor -v
   ```

2. Clear all Flutter caches:
   ```powershell
   flutter pub cache repair
   ```

3. Delete and regenerate ephemeral files:
   ```powershell
   cd work\probationdesk_src\flutter
   Remove-Item -Recurse -Force windows\flutter\ephemeral
   Remove-Item -Recurse -Force .dart_tool
   flutter pub get
   ```

4. Verify the plugin repository:
   - The plugin comes from: https://github.com/rustdesk-org/flutter_gpu_texture_renderer
   - Commit: 08a471bb8ceccdd50483c81cdfa8b81b07b14b87

## Success Indicators

Build is successful when you see:

```
✓ Built build\windows\x64\runner\Release\ProbationDesk.exe
```

The executable will be located at:
```
work\probationdesk_src\flutter\build\windows\x64\runner\Release\ProbationDesk.exe
```

## Need More Help?

Refer to:
- `INSTRUCTIONS_RU.md` - Complete Russian instructions
- `QUICK_START.md` - Quick start guide
- Problem 5 in the Russian instructions for detailed troubleshooting
