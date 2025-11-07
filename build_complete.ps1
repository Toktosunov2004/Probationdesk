# ===================================================================
# Complete Build Script for ProbationDesk
# ===================================================================
# This script performs a complete build from scratch including:
# 1. Generating Flutter Rust Bridge files
# 2. Building Rust library
# 3. Building Flutter Windows application
# ===================================================================

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "ProbationDesk Complete Build Script" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

$ErrorActionPreference = "Stop"

# Navigate to source directory
$sourceDir = "work\probationdesk_src"
if (-not (Test-Path $sourceDir)) {
    Write-Host "Error: Source directory not found: $sourceDir" -ForegroundColor Red
    Write-Host "Please run this script from the Probationdesk root directory" -ForegroundColor Yellow
    exit 1
}

Set-Location $sourceDir
Write-Host "[Step 1/7] Working directory: $(Get-Location)" -ForegroundColor Green
Write-Host ""

# Check if flutter_rust_bridge_codegen is installed
Write-Host "[Step 2/7] Checking flutter_rust_bridge_codegen..." -ForegroundColor Yellow
$codegen = Get-Command flutter_rust_bridge_codegen -ErrorAction SilentlyContinue
if (-not $codegen) {
    Write-Host "flutter_rust_bridge_codegen not found. Installing..." -ForegroundColor Yellow
    cargo install flutter_rust_bridge_codegen --version 1.80.1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Error: Failed to install flutter_rust_bridge_codegen" -ForegroundColor Red
        exit 1
    }
    Write-Host "flutter_rust_bridge_codegen installed successfully" -ForegroundColor Green
} else {
    Write-Host "flutter_rust_bridge_codegen found: $($codegen.Source)" -ForegroundColor Green
}
Write-Host ""

# Generate Flutter Rust Bridge files
Write-Host "[Step 3/7] Generating Flutter Rust Bridge files..." -ForegroundColor Yellow
Write-Host "This will create:" -ForegroundColor Gray
Write-Host "  - src\bridge_generated.rs" -ForegroundColor Gray
Write-Host "  - flutter\lib\generated_bridge.dart" -ForegroundColor Gray
Write-Host ""

flutter_rust_bridge_codegen --rust-input src/flutter_ffi.rs --dart-output flutter/lib/generated_bridge.dart

if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "Error: Failed to generate bridge files" -ForegroundColor Red
    exit 1
}

# Verify generated files exist
if (Test-Path "src\bridge_generated.rs") {
    Write-Host "✓ src\bridge_generated.rs created" -ForegroundColor Green
} else {
    Write-Host "✗ src\bridge_generated.rs missing!" -ForegroundColor Red
    exit 1
}

if (Test-Path "flutter\lib\generated_bridge.dart") {
    Write-Host "✓ flutter\lib\generated_bridge.dart created" -ForegroundColor Green
} else {
    Write-Host "✗ flutter\lib\generated_bridge.dart missing!" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Bridge files generated successfully!" -ForegroundColor Green
Write-Host ""

# Build Rust library
Write-Host "[Step 4/7] Building Rust library with Flutter features..." -ForegroundColor Yellow
Write-Host "This may take 10-15 minutes on first build..." -ForegroundColor Gray
Write-Host ""

$buildStart = Get-Date
cargo build --release --features flutter --lib

if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "Error: Rust library build failed!" -ForegroundColor Red
    exit 1
}

$buildTime = ((Get-Date) - $buildStart).TotalSeconds
Write-Host ""
Write-Host "Rust library built successfully in $([math]::Round($buildTime, 1)) seconds" -ForegroundColor Green
Write-Host ""

# Verify Rust library
$rustLib = "target\release\librustdesk.dll"
if (Test-Path $rustLib) {
    $libInfo = Get-Item $rustLib
    Write-Host "✓ $rustLib ($([math]::Round($libInfo.Length / 1MB, 2)) MB)" -ForegroundColor Green
} else {
    Write-Host "✗ $rustLib not found!" -ForegroundColor Red
    exit 1
}
Write-Host ""

# Navigate to Flutter directory
Set-Location flutter
Write-Host "[Step 5/7] Changed to Flutter directory" -ForegroundColor Green
Write-Host ""

# Get Flutter dependencies
Write-Host "[Step 6/7] Getting Flutter dependencies..." -ForegroundColor Yellow
flutter pub get

if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "Error: Failed to get Flutter dependencies" -ForegroundColor Red
    exit 1
}

Write-Host "Flutter dependencies resolved" -ForegroundColor Green
Write-Host ""

# Build Flutter Windows application
Write-Host "[Step 7/7] Building Flutter Windows application..." -ForegroundColor Yellow
Write-Host "This may take 5-10 minutes..." -ForegroundColor Gray
Write-Host ""

$flutterBuildStart = Get-Date
flutter build windows --release

if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "==================================================" -ForegroundColor Red
    Write-Host "FLUTTER BUILD FAILED!" -ForegroundColor Red
    Write-Host "==================================================" -ForegroundColor Red
    Write-Host ""
    Write-Host "Common issues:" -ForegroundColor Yellow
    Write-Host "1. Plugin errors - Try running: ..\fix_flutter_build.ps1" -ForegroundColor White
    Write-Host "2. Missing dependencies - Check Flutter doctor" -ForegroundColor White
    Write-Host "3. Visual Studio issues - Ensure Build Tools are installed" -ForegroundColor White
    exit 1
}

$flutterBuildTime = ((Get-Date) - $flutterBuildStart).TotalSeconds
Write-Host ""
Write-Host "Flutter application built successfully in $([math]::Round($flutterBuildTime, 1)) seconds" -ForegroundColor Green
Write-Host ""

# Verify executable
$exePath = "build\windows\x64\runner\Release\ProbationDesk.exe"
if (Test-Path $exePath) {
    $exeInfo = Get-Item $exePath
    Write-Host "==================================================" -ForegroundColor Green
    Write-Host "BUILD COMPLETED SUCCESSFULLY!" -ForegroundColor Green
    Write-Host "==================================================" -ForegroundColor Green
    Write-Host ""
    Write-Host "Executable Information:" -ForegroundColor Cyan
    Write-Host "  Location: $exePath" -ForegroundColor White
    Write-Host "  Size: $([math]::Round($exeInfo.Length / 1MB, 2)) MB" -ForegroundColor White
    Write-Host "  Modified: $($exeInfo.LastWriteTime)" -ForegroundColor White
    Write-Host ""
    Write-Host "Total build time: $([math]::Round($buildTime + $flutterBuildTime, 1)) seconds" -ForegroundColor Gray
    Write-Host ""
    Write-Host "To run the application:" -ForegroundColor Cyan
    Write-Host "  cd $(Get-Location)" -ForegroundColor White
    Write-Host "  .\$exePath" -ForegroundColor White
    Write-Host ""
    Write-Host "Or double-click the file in Windows Explorer" -ForegroundColor Gray
    Write-Host ""
} else {
    Write-Host "==================================================" -ForegroundColor Yellow
    Write-Host "BUILD COMPLETED WITH WARNINGS" -ForegroundColor Yellow
    Write-Host "==================================================" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "The build process completed but executable not found at:" -ForegroundColor Yellow
    Write-Host "  $exePath" -ForegroundColor White
    Write-Host ""
    Write-Host "Check build output above for errors" -ForegroundColor Yellow
}

# Return to original directory
Set-Location ..\..
