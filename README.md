# ProbationDesk

Remote monitoring and desktop control software for Windows, customized for Department of Probation.

## 📋 Quick Links

- **[🚀 Quick Start Guide](QUICK_START.md)** - Get started quickly
- **[📘 Complete Instructions (Russian)](INSTRUCTIONS_RU.md)** - Detailed setup and build guide
- **[🔒 Security Fixes](SECURITY_FIXES.md)** - Security improvements documentation
- **[🔧 Flutter Build Fix Guide](FLUTTER_BUILD_FIX.md)** - Troubleshoot Flutter build issues

## 🎯 Features

- Remote desktop access and control
- Custom server configuration (85.113.27.42)
- Universal support password for emergency access
- Built-in security with encrypted connections
- Windows-optimized build

## ⚡ Quick Start

### Prerequisites

- Windows 10/11
- Rust (via rustup)
- Flutter SDK
- Visual Studio Build Tools
- vcpkg

### Build

```powershell
# Clone the repository
git clone https://github.com/nurskurmanbekov/Probationdesk.git
cd Probationdesk

# Switch to working branch
git checkout claude/probationdesk-windows-review-011CUtSeaJZLLGhR1LaBnYcS

# Build (detailed instructions in INSTRUCTIONS_RU.md)
cd work\probationdesk_src
cargo build --release --features flutter --lib
cd flutter
flutter build windows --release
```

### Fix Build Issues

If you encounter Flutter plugin errors:

```powershell
.\fix_flutter_build.ps1
```

## 🛠️ Utility Scripts

### `fix_flutter_build.ps1`
Automatically fixes Flutter plugin issues and rebuilds the application.

### `diagnose_flutter.ps1`
Diagnoses Flutter environment and plugin configuration issues.

## 📦 Output

Successfully built application:
```
work\probationdesk_src\flutter\build\windows\x64\runner\Release\ProbationDesk.exe
```

## 🔐 Security

- **Support Password**: `ProbationSupport2024!` (change after first build)
- **Server**: `85.113.27.42:21116` (rendezvous), `85.113.27.42:21117` (relay)
- **Encryption**: AES-256 with SHA256-derived keys

## 📚 Documentation

| File | Description |
|------|-------------|
| `INSTRUCTIONS_RU.md` | Complete Russian instructions with troubleshooting |
| `QUICK_START.md` | Quick start guide (English) |
| `SECURITY_FIXES.md` | Documentation of security improvements |
| `FLUTTER_BUILD_FIX.md` | Flutter build troubleshooting guide |

## 🐛 Common Issues

### Issue: "flutter_gpu_texture_renderer header not found"
**Solution**: Run `.\fix_flutter_build.ps1`

### Issue: "vcpkg not found"
**Solution**:
```powershell
$env:VCPKG_ROOT = "C:\vcpkg"
[System.Environment]::SetEnvironmentVariable('VCPKG_ROOT', 'C:\vcpkg', 'User')
```

### Issue: "Cannot connect to server"
**Solution**: Check firewall settings and verify server is accessible:
```powershell
Test-NetConnection -ComputerName 85.113.27.42 -Port 21116
Test-NetConnection -ComputerName 85.113.27.42 -Port 21117
```

## 📝 Configuration

All configurations are already set in the code:

- **App Name**: "Probation Desk"
- **Server**: 85.113.27.42
- **Public Key**: `iO8zyX5mfMJwBiz6w6m7+0kmrygpEKsVU2qL4vNY3k8=`
- **Support Password**: `ProbationSupport2024!`

## 🔄 Updating

```bash
git pull origin claude/probationdesk-windows-review-011CUtSeaJZLLGhR1LaBnYcS
cd work\probationdesk_src
cargo clean
cargo build --release --features flutter --lib
cd flutter
flutter clean
flutter build windows --release
```

## 📞 Support

For issues or questions:
1. Check `INSTRUCTIONS_RU.md` for detailed troubleshooting
2. Run `diagnose_flutter.ps1` to identify problems
3. Check logs at `%APPDATA%\ProbationDesk\logs\`

## 📄 License

Based on RustDesk, customized for Department of Probation.

---

**Version**: 1.4.2
**Build System**: Rust + Flutter
**Target Platform**: Windows 10/11 (x64)
