# ===================================================================
# Flutter Plugin Diagnostic Script for ProbationDesk
# ===================================================================
# This script diagnoses Flutter plugin issues
# ===================================================================

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "ProbationDesk Flutter Diagnostic Script" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

# Navigate to Flutter directory
$flutterDir = "work\probationdesk_src\flutter"
if (-not (Test-Path $flutterDir)) {
    Write-Host "Error: Flutter directory not found at: $flutterDir" -ForegroundColor Red
    exit 1
}

Set-Location $flutterDir

# Check Flutter version
Write-Host "[1] Flutter Version:" -ForegroundColor Yellow
flutter --version
Write-Host ""

# Check Dart version
Write-Host "[2] Dart Version:" -ForegroundColor Yellow
dart --version
Write-Host ""

# Check pub cache location
Write-Host "[3] Pub Cache Location:" -ForegroundColor Yellow
$pubCache = "$env:LOCALAPPDATA\Pub\Cache"
Write-Host "  $pubCache" -ForegroundColor White
if (Test-Path $pubCache) {
    $cacheSize = (Get-ChildItem $pubCache -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB
    Write-Host "  Size: $([math]::Round($cacheSize, 2)) MB" -ForegroundColor Gray
} else {
    Write-Host "  Not found!" -ForegroundColor Red
}
Write-Host ""

# Check Git plugins in cache
Write-Host "[4] Git Plugins in Cache:" -ForegroundColor Yellow
$gitPluginsDir = "$pubCache\git"
if (Test-Path $gitPluginsDir) {
    Get-ChildItem $gitPluginsDir -Directory | ForEach-Object {
        Write-Host "  - $($_.Name)" -ForegroundColor Gray
    }

    # Check flutter_gpu_texture_renderer specifically
    $gpuPluginCache = Get-ChildItem $gitPluginsDir -Directory | Where-Object { $_.Name -like "*flutter_gpu_texture_renderer*" }
    if ($gpuPluginCache) {
        Write-Host ""
        Write-Host "  flutter_gpu_texture_renderer found:" -ForegroundColor Green
        foreach ($plugin in $gpuPluginCache) {
            Write-Host "    Path: $($plugin.FullName)" -ForegroundColor White
            $windowsDir = Join-Path $plugin.FullName "windows"
            if (Test-Path $windowsDir) {
                Write-Host "    - Windows directory: EXISTS" -ForegroundColor Green
                $includeDir = Join-Path $windowsDir "include"
                if (Test-Path $includeDir) {
                    Write-Host "    - Include directory: EXISTS" -ForegroundColor Green
                } else {
                    Write-Host "    - Include directory: MISSING" -ForegroundColor Red
                }
            } else {
                Write-Host "    - Windows directory: MISSING" -ForegroundColor Red
            }
        }
    } else {
        Write-Host "  flutter_gpu_texture_renderer: NOT FOUND" -ForegroundColor Red
    }
} else {
    Write-Host "  Git plugins directory not found" -ForegroundColor Red
}
Write-Host ""

# Check plugin symlinks
Write-Host "[5] Plugin Symlinks:" -ForegroundColor Yellow
$symlinkDir = "windows\flutter\ephemeral\.plugin_symlinks"
if (Test-Path $symlinkDir) {
    Write-Host "  Directory exists: $symlinkDir" -ForegroundColor Green
    Get-ChildItem $symlinkDir -Directory -ErrorAction SilentlyContinue | ForEach-Object {
        Write-Host "  - $($_.Name)" -ForegroundColor Gray
    }

    # Check flutter_gpu_texture_renderer symlink
    $gpuSymlink = Join-Path $symlinkDir "flutter_gpu_texture_renderer"
    if (Test-Path $gpuSymlink) {
        Write-Host ""
        Write-Host "  flutter_gpu_texture_renderer symlink:" -ForegroundColor Green
        Write-Host "    Path: $gpuSymlink" -ForegroundColor White

        # Check for header file
        $headerFile = "$gpuSymlink\windows\include\flutter_gpu_texture_renderer\flutter_gpu_texture_renderer_plugin_c_api.h"
        if (Test-Path $headerFile) {
            Write-Host "    - Header file: EXISTS" -ForegroundColor Green
            $headerInfo = Get-Item $headerFile
            Write-Host "      Size: $($headerInfo.Length) bytes" -ForegroundColor Gray
            Write-Host "      Modified: $($headerInfo.LastWriteTime)" -ForegroundColor Gray
        } else {
            Write-Host "    - Header file: MISSING" -ForegroundColor Red
            Write-Host "      Expected at: $headerFile" -ForegroundColor Gray
        }

        # Check for source file
        $sourceFile = "$gpuSymlink\windows\flutter_gpu_texture_renderer_plugin_c_api.cpp"
        if (Test-Path $sourceFile) {
            Write-Host "    - Source file: EXISTS" -ForegroundColor Green
        } else {
            Write-Host "    - Source file: MISSING" -ForegroundColor Red
        }
    } else {
        Write-Host ""
        Write-Host "  flutter_gpu_texture_renderer: NOT FOUND" -ForegroundColor Red
    }
} else {
    Write-Host "  Directory does not exist yet (will be created during build)" -ForegroundColor Gray
}
Write-Host ""

# Check pubspec.lock
Write-Host "[6] Dependency Lock File:" -ForegroundColor Yellow
if (Test-Path "pubspec.lock") {
    Write-Host "  pubspec.lock exists" -ForegroundColor Green
    $lockContent = Get-Content "pubspec.lock" -Raw
    if ($lockContent -match "flutter_gpu_texture_renderer:") {
        Write-Host "  - flutter_gpu_texture_renderer: LOCKED" -ForegroundColor Green
    } else {
        Write-Host "  - flutter_gpu_texture_renderer: NOT FOUND" -ForegroundColor Red
    }
} else {
    Write-Host "  pubspec.lock: NOT FOUND" -ForegroundColor Red
}
Write-Host ""

# Check .dart_tool
Write-Host "[7] Dart Tool Directory:" -ForegroundColor Yellow
if (Test-Path ".dart_tool") {
    Write-Host "  .dart_tool exists" -ForegroundColor Green
    $dartToolSize = (Get-ChildItem ".dart_tool" -Recurse -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum / 1MB
    Write-Host "  Size: $([math]::Round($dartToolSize, 2)) MB" -ForegroundColor Gray
} else {
    Write-Host "  .dart_tool: NOT FOUND" -ForegroundColor Red
}
Write-Host ""

# Check build directory
Write-Host "[8] Build Directory:" -ForegroundColor Yellow
if (Test-Path "build") {
    Write-Host "  build exists" -ForegroundColor Green
    if (Test-Path "build\windows") {
        Write-Host "  - Windows build exists" -ForegroundColor Green
    } else {
        Write-Host "  - Windows build: NOT FOUND" -ForegroundColor Gray
    }
} else {
    Write-Host "  build: NOT FOUND (clean state)" -ForegroundColor Gray
}
Write-Host ""

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "Diagnostic completed!" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "RECOMMENDATIONS:" -ForegroundColor Yellow
Write-Host ""

if (-not (Test-Path "pubspec.lock")) {
    Write-Host "1. Run: flutter pub get" -ForegroundColor White
}

$gpuPluginFound = $false
if (Test-Path "$pubCache\git") {
    $gpuPluginCache = Get-ChildItem "$pubCache\git" -Directory | Where-Object { $_.Name -like "*flutter_gpu_texture_renderer*" }
    if ($gpuPluginCache) {
        $gpuPluginFound = $true
    }
}

if (-not $gpuPluginFound) {
    Write-Host "2. flutter_gpu_texture_renderer not in cache - run: flutter pub get" -ForegroundColor White
}

Write-Host ""
Write-Host "To fix build issues, run: .\fix_flutter_build.ps1" -ForegroundColor Cyan
