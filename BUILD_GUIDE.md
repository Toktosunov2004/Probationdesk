# 🏗️ ProbationDesk Build Guide

## ⚠️ Important: You MUST Generate Bridge Files First!

Before building ProbationDesk, you **MUST** generate Flutter Rust Bridge files. Without these files, the build will fail with errors like:

```
error[E0583]: file not found for module `bridge_generated`
Error when reading 'lib/generated_bridge.dart': File not found
'RustdeskImpl' isn't a type
```

## 🚀 Quick Build (Recommended)

### Use the Automated Script

```powershell
# Navigate to project root
cd Desktop\Probationdesk

# Run the complete build script
.\build_complete.ps1
```

This script does **EVERYTHING** for you:
1. ✅ Checks and installs `flutter_rust_bridge_codegen` if needed
2. ✅ Generates `bridge_generated.rs` and `generated_bridge.dart`
3. ✅ Builds Rust library with Flutter features
4. ✅ Builds Flutter Windows application
5. ✅ Shows where the `ProbationDesk.exe` file is located

## 📋 Manual Build Steps

If you prefer to build manually or troubleshoot issues:

### Step 1: Install flutter_rust_bridge_codegen (Once)

```powershell
cargo install flutter_rust_bridge_codegen --version 1.80.1
```

### Step 2: Generate Bridge Files (Every time)

```powershell
cd Desktop\Probationdesk\work\probationdesk_src

flutter_rust_bridge_codegen --rust-input src/flutter_ffi.rs --dart-output flutter/lib/generated_bridge.dart
```

**You should see:**
```
Generated bridge files:
  - src\bridge_generated.rs
  - flutter\lib\generated_bridge.dart
```

### Step 3: Build Rust Library

```powershell
cargo build --release --features flutter --lib
```

**This will take 10-15 minutes on first build.**

### Step 4: Build Flutter Application

```powershell
cd flutter
flutter pub get
flutter build windows --release
```

**This will take 5-10 minutes.**

### Step 5: Find Your Executable

The finished application will be at:
```
flutter\build\windows\x64\runner\Release\ProbationDesk.exe
```

## 🐛 Common Build Errors

### Error: "file not found for module `bridge_generated`"

**Cause:** Bridge files were not generated.

**Solution:**
```powershell
cd Desktop\Probationdesk\work\probationdesk_src
flutter_rust_bridge_codegen --rust-input src/flutter_ffi.rs --dart-output flutter/lib/generated_bridge.dart
cargo build --release --features flutter --lib
```

### Error: "Error when reading 'lib/generated_bridge.dart'"

**Cause:** Dart bridge file is missing.

**Solution:** Same as above - generate bridge files first.

### Error: "flutter_gpu_texture_renderer_plugin_c_api.h not found"

**Cause:** Flutter plugin cache issue.

**Solution:**
```powershell
cd Desktop\Probationdesk
.\fix_flutter_build.ps1
```

### Error: "'RustdeskImpl' isn't a type"

**Cause:** Bridge files are outdated or missing.

**Solution:** Regenerate bridge files and rebuild:
```powershell
cd Desktop\Probationdesk\work\probationdesk_src
flutter_rust_bridge_codegen --rust-input src/flutter_ffi.rs --dart-output flutter/lib/generated_bridge.dart
cargo clean
cargo build --release --features flutter --lib
cd flutter
flutter clean
flutter build windows --release
```

## 📦 Build Output Locations

After successful build:

- **Rust library**: `work\probationdesk_src\target\release\librustdesk.dll`
- **Flutter executable**: `work\probationdesk_src\flutter\build\windows\x64\runner\Release\ProbationDesk.exe`
- **Generated bridge files**:
  - `work\probationdesk_src\src\bridge_generated.rs`
  - `work\probationdesk_src\flutter\lib\generated_bridge.dart`

## 🔄 Rebuilding After Code Changes

If you change Rust code:

```powershell
cd Desktop\Probationdesk\work\probationdesk_src

# Regenerate bridge files if you changed flutter_ffi.rs
flutter_rust_bridge_codegen --rust-input src/flutter_ffi.rs --dart-output flutter/lib/generated_bridge.dart

# Rebuild Rust
cargo build --release --features flutter --lib

# Rebuild Flutter
cd flutter
flutter build windows --release
```

**OR** simply run:

```powershell
cd Desktop\Probationdesk
.\build_complete.ps1
```

## 💡 Pro Tips

1. **First time building?** Use `build_complete.ps1` - it handles everything automatically.

2. **Build failed?** Try:
   ```powershell
   .\diagnose_flutter.ps1  # Check what's wrong
   .\fix_flutter_build.ps1  # Fix Flutter issues
   .\build_complete.ps1     # Complete rebuild
   ```

3. **Faster rebuilds:** The first build takes 15-20 minutes. Subsequent builds take only 2-5 minutes.

4. **Clean build:** If things go wrong:
   ```powershell
   cd work\probationdesk_src
   cargo clean
   cd flutter
   flutter clean
   cd ..\..
   .\build_complete.ps1
   ```

5. **Check dependencies:**
   ```powershell
   rustc --version   # Should be 1.70+
   flutter --version # Should be 3.1+
   cargo --version   # Should be installed
   ```

## 📚 Additional Resources

- **Complete Russian Instructions**: [INSTRUCTIONS_RU.md](INSTRUCTIONS_RU.md)
- **Flutter Build Issues**: [FLUTTER_BUILD_FIX.md](FLUTTER_BUILD_FIX.md)
- **Quick Start**: [QUICK_START.md](QUICK_START.md)
- **Security Info**: [SECURITY_FIXES.md](SECURITY_FIXES.md)

## ❓ Still Having Issues?

1. Check that all prerequisites are installed:
   - Rust (rustup)
   - Flutter SDK
   - Visual Studio Build Tools
   - vcpkg with required libraries

2. Verify environment variables:
   ```powershell
   echo $env:VCPKG_ROOT   # Should be C:\vcpkg
   ```

3. Try diagnostic script:
   ```powershell
   .\diagnose_flutter.ps1
   ```

4. Check detailed logs in:
   - Rust build: Look at console output
   - Flutter build: Check `flutter\build\windows\x64\build.log`

## ✅ Successful Build Checklist

After building, verify:

- [ ] `librustdesk.dll` exists in `target\release\`
- [ ] `ProbationDesk.exe` exists in `flutter\build\windows\x64\runner\Release\`
- [ ] `bridge_generated.rs` exists in `src\`
- [ ] `generated_bridge.dart` exists in `flutter\lib\`
- [ ] Application launches without errors
- [ ] Can connect to server at 85.113.27.42

---

**Happy Building! 🎉**

For questions, check [INSTRUCTIONS_RU.md](INSTRUCTIONS_RU.md) for detailed troubleshooting.
