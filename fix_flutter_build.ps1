# ===================================================================
# Fix Flutter Build Script for ProbationDesk
# ===================================================================
# This script fixes the flutter_gpu_texture_renderer plugin error
# and rebuilds the Flutter Windows application
# ===================================================================

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "ProbationDesk Flutter Build Fix Script" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

# Navigate to Flutter directory
$flutterDir = "work\probationdesk_src\flutter"
if (-not (Test-Path $flutterDir)) {
    Write-Host "Error: Flutter directory not found at: $flutterDir" -ForegroundColor Red
    exit 1
}

Set-Location $flutterDir
Write-Host "[1/8] Current directory: $(Get-Location)" -ForegroundColor Green

# Step 1: Clean Flutter build cache
Write-Host ""
Write-Host "[2/8] Cleaning Flutter build cache..." -ForegroundColor Yellow
flutter clean
if ($LASTEXITCODE -ne 0) {
    Write-Host "Warning: Flutter clean had issues, continuing..." -ForegroundColor Yellow
}

# Step 2: Remove pub cache for problematic plugins
Write-Host ""
Write-Host "[3/8] Clearing pub cache for Git plugins..." -ForegroundColor Yellow
$pubCacheDir = "$env:LOCALAPPDATA\Pub\Cache\git"
if (Test-Path $pubCacheDir) {
    Write-Host "Removing: $pubCacheDir" -ForegroundColor Gray
    Remove-Item -Path "$pubCacheDir\flutter_gpu_texture_renderer*" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "$pubCacheDir\texture_rgba_renderer*" -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Git plugin cache cleared" -ForegroundColor Green
} else {
    Write-Host "Pub cache directory not found, skipping..." -ForegroundColor Gray
}

# Step 3: Delete pubspec.lock to force fresh dependency resolution
Write-Host ""
Write-Host "[4/8] Removing pubspec.lock..." -ForegroundColor Yellow
if (Test-Path "pubspec.lock") {
    Remove-Item "pubspec.lock" -Force
    Write-Host "pubspec.lock removed" -ForegroundColor Green
}

# Step 4: Fetch fresh dependencies
Write-Host ""
Write-Host "[5/8] Fetching fresh dependencies (this may take a few minutes)..." -ForegroundColor Yellow
flutter pub get
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: flutter pub get failed!" -ForegroundColor Red
    exit 1
}
Write-Host "Dependencies fetched successfully" -ForegroundColor Green

# Step 5: Upgrade dependencies (optional but recommended)
Write-Host ""
Write-Host "[6/8] Upgrading dependencies..." -ForegroundColor Yellow
flutter pub upgrade
if ($LASTEXITCODE -ne 0) {
    Write-Host "Warning: flutter pub upgrade had issues, continuing..." -ForegroundColor Yellow
}

# Step 6: Verify plugin symlinks
Write-Host ""
Write-Host "[7/8] Checking plugin symlinks..." -ForegroundColor Yellow
$pluginSymlinks = "windows\flutter\ephemeral\.plugin_symlinks"
if (Test-Path $pluginSymlinks) {
    Write-Host "Plugin symlinks exist at: $pluginSymlinks" -ForegroundColor Green
    $gpuPlugin = "$pluginSymlinks\flutter_gpu_texture_renderer"
    if (Test-Path $gpuPlugin) {
        Write-Host "  - flutter_gpu_texture_renderer: OK" -ForegroundColor Green
        # Check if header file exists
        $headerFile = "$gpuPlugin\windows\include\flutter_gpu_texture_renderer\flutter_gpu_texture_renderer_plugin_c_api.h"
        if (Test-Path $headerFile) {
            Write-Host "  - Header file exists: OK" -ForegroundColor Green
        } else {
            Write-Host "  - WARNING: Header file still missing!" -ForegroundColor Red
            Write-Host "  - Expected at: $headerFile" -ForegroundColor Gray
        }
    } else {
        Write-Host "  - WARNING: flutter_gpu_texture_renderer plugin not found!" -ForegroundColor Red
    }
} else {
    Write-Host "Plugin symlinks directory not created yet (will be created during build)" -ForegroundColor Gray
}

# Step 7: Build the Flutter Windows application
Write-Host ""
Write-Host "[8/8] Building Flutter Windows application (this will take several minutes)..." -ForegroundColor Yellow
Write-Host "Build started at: $(Get-Date -Format 'HH:mm:ss')" -ForegroundColor Gray
Write-Host ""

flutter build windows --release

if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "==================================================" -ForegroundColor Red
    Write-Host "BUILD FAILED!" -ForegroundColor Red
    Write-Host "==================================================" -ForegroundColor Red
    Write-Host ""
    Write-Host "If the error persists, try the following:" -ForegroundColor Yellow
    Write-Host "1. Update Flutter: flutter upgrade" -ForegroundColor White
    Write-Host "2. Clear all Flutter caches:" -ForegroundColor White
    Write-Host "   flutter pub cache repair" -ForegroundColor Gray
    Write-Host "3. Manually delete:" -ForegroundColor White
    Write-Host "   - windows\flutter\ephemeral\" -ForegroundColor Gray
    Write-Host "   - .dart_tool\" -ForegroundColor Gray
    Write-Host "4. Run this script again" -ForegroundColor White
    Write-Host ""
    exit 1
}

Write-Host ""
Write-Host "==================================================" -ForegroundColor Green
Write-Host "BUILD SUCCESSFUL!" -ForegroundColor Green
Write-Host "==================================================" -ForegroundColor Green
Write-Host ""
Write-Host "Built at: $(Get-Date -Format 'HH:mm:ss')" -ForegroundColor Gray
Write-Host ""

# Check if executable was created
$exePath = "build\windows\x64\runner\Release\ProbationDesk.exe"
if (Test-Path $exePath) {
    $exeInfo = Get-Item $exePath
    Write-Host "Executable created successfully!" -ForegroundColor Green
    Write-Host "  Location: $exePath" -ForegroundColor White
    Write-Host "  Size: $([math]::Round($exeInfo.Length / 1MB, 2)) MB" -ForegroundColor White
    Write-Host "  Modified: $($exeInfo.LastWriteTime)" -ForegroundColor White
    Write-Host ""
    Write-Host "You can now run: .\$exePath" -ForegroundColor Cyan
} else {
    Write-Host "Warning: Executable not found at expected location: $exePath" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "Script completed successfully!" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
