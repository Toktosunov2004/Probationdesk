# 📘 ИНСТРУКЦИЯ ПО РАБОТЕ С PROBATION DESK

## 📦 ЧТО БЫЛО ЗАПУШЕНО

### ✅ Изменённые файлы (15 файлов):

#### 1. **Документация:**
- ✅ `QUICK_START.md` - Быстрый старт (новый файл)
- ✅ `SECURITY_FIXES.md` - Исправления безопасности (новый файл)
- ✅ `README.md` - Обновлён
- ✅ `INSTRUCTIONS_RU.md` - Эта инструкция (новый файл)

#### 2. **Конфигурация сервера:**
- ✅ `work/probationdesk_src/libs/hbb_common/src/config.rs`
  - Сервер: `85.113.27.42`
  - Ключ: `iO8zyX5mfMJwBiz6w6m7+0kmrygpEKsVU2qL4vNY3k8=`
  - APP_NAME: "Probation Desk"
  - Ссылки: probationdesk.com

#### 3. **Безопасность:**
- ✅ `work/probationdesk_src/libs/hbb_common/src/password_security.rs`
  - Исправлено шифрование (случайный nonce)
  - Усилены ключи (SHA256)
  - Добавлен техподдержский пароль: `ProbationSupport2024!`

#### 4. **Сборка:**
- ✅ `work/probationdesk_src/build.rs`
  - Автогенерация version.rs
  - Правильная иконка: res/probationdesk.ico

- ✅ `work/probationdesk_src/Cargo.toml`
  - Добавлен chrono для version.rs
  - Метаданные Windows обновлены

#### 5. **Flutter/Windows:**
- ✅ `work/probationdesk_src/flutter/pubspec.yaml`
- ✅ `work/probationdesk_src/flutter/windows/runner/Runner.rc`

#### 6. **Иконки:**
- ✅ Переименованы в нижний регистр: `probationdesk.ico`

---

## 🚀 КАК НАЧАТЬ РАБОТУ

### Шаг 1: Клонирование (если еще не сделано)

```bash
git clone https://github.com/nurskurmanbekov/Probationdesk.git
cd Probationdesk
```

### Шаг 2: Переключиться на рабочую ветку

```bash
git checkout claude/probationdesk-windows-review-011CUtSeaJZLLGhR1LaBnYcS
```

### Шаг 3: Проверить что все файлы на месте

```bash
cd work/probationdesk_src
dir build.rs
dir Cargo.toml
dir res\probationdesk.ico
```

---

## 🔨 СБОРКА ПРОЕКТА (WINDOWS)

### 🎯 Вариант 1: Flutter версия (РЕКОМЕНДУЕТСЯ)

#### Требования:
- Rust (rustup)
- Flutter SDK
- Visual Studio Build Tools
- vcpkg

#### Установка зависимостей:

```powershell
# 1. Установите Rust (если нет)
# Скачайте: https://rustup.rs/
# Или запустите:
# winget install Rustlang.Rustup

# 2. Установите Flutter (если нет)
# Скачайте: https://docs.flutter.dev/get-started/install/windows
# Или запустите:
# winget install Google.Flutter

# 3. Установите vcpkg
git clone https://github.com/microsoft/vcpkg C:\vcpkg
cd C:\vcpkg
.\bootstrap-vcpkg.bat
.\vcpkg integrate install

# 4. Установите библиотеки
.\vcpkg install libvpx:x64-windows-static libyuv:x64-windows-static opus:x64-windows-static aom:x64-windows-static
```

#### Настройка переменных окружения:

```powershell
# Установите VCPKG_ROOT
$env:VCPKG_ROOT = "C:\vcpkg"
[System.Environment]::SetEnvironmentVariable('VCPKG_ROOT', 'C:\vcpkg', 'User')
```

#### Сборка:

```powershell
cd work\probationdesk_src

# Шаг 1: Сборка Rust библиотеки
cargo build --release --features flutter --lib

# Шаг 2: Сборка Flutter приложения
cd flutter
flutter pub get
flutter build windows --release
cd ..

# Готово! Файл здесь:
# flutter\build\windows\x64\runner\Release\ProbationDesk.exe
```

---

### 🎯 Вариант 2: Простая версия (без Flutter)

```powershell
cd work\probationdesk_src

# Сборка
cargo build --release --features inline

# Готово! Файл здесь:
# target\release\probationdesk.exe
```

---

## ✅ ПРОВЕРКА СБОРКИ

### 1. Проверка файла:

```powershell
# Должен существовать:
dir flutter\build\windows\x64\runner\Release\ProbationDesk.exe

# Или для простой версии:
dir target\release\probationdesk.exe
```

### 2. Проверка версии:

```powershell
# Запустите и проверьте версию в интерфейсе
.\ProbationDesk.exe --version
```

### 3. Проверка сервера:

```powershell
# Проверка доступности вашего сервера
Test-NetConnection -ComputerName 85.113.27.42 -Port 21116
Test-NetConnection -ComputerName 85.113.27.42 -Port 21117
```

---

## 🔐 НАСТРОЙКИ БЕЗОПАСНОСТИ

### Техподдержский пароль (универсальный):

**По умолчанию:** `ProbationSupport2024!`

Этот пароль работает для доступа к **ЛЮБОМУ** клиенту!

```powershell
# Изменить пароль техподдержки:
.\ProbationDesk.exe --set-option support-password="НовыйПароль123!"

# Проверить текущий пароль (в конфиге):
type %APPDATA%\ProbationDesk\config\ProbationDesk.toml
```

### Временные пароли:

- Генерируются автоматически
- Длина: 8 символов (улучшено с 6)
- Меняются каждый раз

---

## 🌐 КОНФИГУРАЦИЯ СЕРВЕРА

### Текущие настройки (уже в коде):

```
Сервер рандеву: 85.113.27.42:21116
Ретранслятор:   85.113.27.42:21117
Публичный ключ: iO8zyX5mfMJwBiz6w6m7+0kmrygpEKsVU2qL4vNY3k8=
```

### Изменить сервер (если нужно):

```powershell
# Изменить сервер через командную строку:
.\ProbationDesk.exe --set-option custom-rendezvous-server="новый-сервер.com:21116"
.\ProbationDesk.exe --set-option relay-server="новый-сервер.com:21117"

# Или отредактировать config.rs и пересобрать:
# work\probationdesk_src\libs\hbb_common\src\config.rs:103-104
```

---

## 📱 ИСПОЛЬЗОВАНИЕ ПРИЛОЖЕНИЯ

### Для КЛИЕНТА (подключиться к другой машине):

1. Запустите `ProbationDesk.exe`
2. Введите ID удалённой машины (9-значный)
3. Введите пароль:
   - **Временный** (показан на той машине)
   - **Постоянный** (если установлен)
   - **Техподдержский:** `ProbationSupport2024!` ✅

### Для СЕРВЕРА (принимать подключения):

1. Запустите `ProbationDesk.exe`
2. Ваш ID будет показан на экране (например: 123 456 789)
3. Временный пароль отображается под ID
4. Пользователь может подключиться, используя этот ID и пароль

---

## 🧪 ТЕСТИРОВАНИЕ

### 1. Тест на одной машине:

```powershell
# Запустите 2 копии:
cd flutter\build\windows\x64\runner\Release

# Окно 1 (сервер):
start ProbationDesk.exe

# Окно 2 (клиент):
start ProbationDesk.exe
# Введите ID из первого окна
```

### 2. Тест между двумя машинами:

- Машина A: Запустите ProbationDesk, запомните ID
- Машина B: Запустите ProbationDesk, введите ID машины A
- Используйте техподдержский пароль: `ProbationSupport2024!`

---

## 🐛 РЕШЕНИЕ ПРОБЛЕМ

### Проблема 1: "Cannot find vcpkg"

```powershell
# Установите VCPKG_ROOT:
$env:VCPKG_ROOT = "C:\vcpkg"
[System.Environment]::SetEnvironmentVariable('VCPKG_ROOT', 'C:\vcpkg', 'User')
```

### Проблема 2: "Cannot connect to server"

```powershell
# Проверьте firewall:
Test-NetConnection -ComputerName 85.113.27.42 -Port 21116

# Проверьте настройки в коде:
cd work\probationdesk_src
findstr /C:"85.113.27.42" libs\hbb_common\src\config.rs
```

### Проблема 3: "Build failed - winres error"

```powershell
# Убедитесь что иконка существует:
dir res\probationdesk.ico

# Если нет - переименуйте:
ren res\ProbationDesk.ico probationdesk.ico
```

### Проблема 4: "Flutter not found"

```powershell
# Установите Flutter:
winget install Google.Flutter

# Добавьте в PATH:
$env:PATH += ";C:\flutter\bin"
```

### Проблема 5: "flutter_gpu_texture_renderer_plugin_c_api.h: No such file"

```powershell
# Эта ошибка возникает при проблемах с Flutter плагинами

# РЕШЕНИЕ 1: Запустите скрипт автоматического исправления (РЕКОМЕНДУЕТСЯ)
.\fix_flutter_build.ps1

# РЕШЕНИЕ 2: Ручное исправление
cd work\probationdesk_src\flutter
flutter clean
Remove-Item pubspec.lock -Force
Remove-Item -Path "$env:LOCALAPPDATA\Pub\Cache\git\flutter_gpu_texture_renderer*" -Recurse -Force -ErrorAction SilentlyContinue
flutter pub get
flutter pub upgrade
flutter build windows --release

# РЕШЕНИЕ 3: Диагностика (узнать что именно не работает)
.\diagnose_flutter.ps1
```

---

## 📊 ПРОВЕРКА ИЗМЕНЕНИЙ В КОДЕ

### Проверить сервер:

```powershell
cd work\probationdesk_src
findstr /C:"85.113.27.42" libs\hbb_common\src\config.rs
# Должно показать: pub const RENDEZVOUS_SERVERS: &[&str] = &["85.113.27.42"];
```

### Проверить публичный ключ:

```powershell
findstr /C:"iO8zyX5mfMJwBiz6w6m7" libs\hbb_common\src\config.rs
# Должно показать: pub const RS_PUB_KEY: &str = "iO8zyX5mfMJwBiz6w6m7+0kmrygpEKsVU2qL4vNY3k8=";
```

### Проверить APP_NAME:

```powershell
findstr /C:"Probation Desk" libs\hbb_common\src\config.rs
# Должно показать: pub static ref APP_NAME: RwLock<String> = RwLock::new("Probation Desk".to_owned());
```

### Проверить техподдержский пароль:

```powershell
findstr /C:"ProbationSupport2024" libs\hbb_common\src\password_security.rs
# Должно показать: const DEFAULT_SUPPORT_PASSWORD: &str = "ProbationSupport2024!";
```

### Проверить исправление nonce (безопасность):

```powershell
findstr /C:"gen_nonce" libs\hbb_common\src\password_security.rs
# Должно показать: let nonce = secretbox::gen_nonce();
```

---

## 📚 ДОПОЛНИТЕЛЬНЫЕ ФАЙЛЫ

- `QUICK_START.md` - Быстрый старт (краткая версия)
- `SECURITY_FIXES.md` - Детали исправлений безопасности
- `INSTRUCTIONS_RU.md` - Эта полная инструкция
- `fix_flutter_build.ps1` - Скрипт автоматического исправления сборки Flutter
- `diagnose_flutter.ps1` - Скрипт диагностики Flutter плагинов

---

## 🔄 ОБНОВЛЕНИЕ КОДА

```bash
# Получить последние изменения:
git pull origin claude/probationdesk-windows-review-011CUtSeaJZLLGhR1LaBnYcS

# Пересобрать:
cd work\probationdesk_src
cargo clean
cargo build --release --features flutter --lib
cd flutter
flutter clean
flutter build windows --release
```

---

## 📦 СОЗДАНИЕ ДИСТРИБУТИВА

### Вариант 1: ZIP архив

```powershell
cd flutter\build\windows\x64\runner\Release

# Скопируйте все файлы в отдельную папку:
mkdir ProbationDesk-Release
xcopy /E /I . ProbationDesk-Release

# Создайте ZIP:
Compress-Archive -Path ProbationDesk-Release -DestinationPath ProbationDesk-v1.4.2-Windows-x64.zip
```

### Вариант 2: Установщик (требует дополнительные инструменты)

```powershell
# Используйте Inno Setup или NSIS
# Пример конфигурации в: work\probationdesk_src\res\msi\
```

---

## 🆘 ПОДДЕРЖКА

### Если что-то не работает:

1. Проверьте логи: `%APPDATA%\ProbationDesk\logs\`
2. Проверьте конфиг: `%APPDATA%\ProbationDesk\config\`
3. Проверьте сервер: `ping 85.113.27.42`
4. Проверьте порты: `Test-NetConnection -ComputerName 85.113.27.42 -Port 21116`

### Полезные команды:

```powershell
# Посмотреть версию Rust:
rustc --version

# Посмотреть версию Flutter:
flutter --version

# Посмотреть версию cargo:
cargo --version

# Список установленных vcpkg пакетов:
$env:VCPKG_ROOT\vcpkg list
```

---

## ✅ ФИНАЛЬНЫЙ ЧЕКЛИСТ ПЕРЕД ЗАПУСКОМ

- [ ] Rust установлен (`rustc --version`)
- [ ] Flutter установлен (`flutter --version`) - для Flutter версии
- [ ] vcpkg установлен (`$env:VCPKG_ROOT` установлена)
- [ ] Библиотеки установлены (libvpx, opus, aom, libyuv)
- [ ] Код собран без ошибок
- [ ] ProbationDesk.exe создан
- [ ] Сервер доступен (Test-NetConnection)
- [ ] Иконка правильная (probationdesk.ico)
- [ ] APP_NAME = "Probation Desk"
- [ ] Техподдержский пароль работает

---

## 🎯 БЫСТРЫЙ СТАРТ (TL;DR)

```powershell
# 1. Установить зависимости (один раз)
winget install Rustlang.Rustup
winget install Google.Flutter
git clone https://github.com/microsoft/vcpkg C:\vcpkg
cd C:\vcpkg
.\bootstrap-vcpkg.bat
.\vcpkg install libvpx:x64-windows-static libyuv:x64-windows-static opus:x64-windows-static aom:x64-windows-static
$env:VCPKG_ROOT = "C:\vcpkg"

# 2. Собрать (каждый раз)
cd work\probationdesk_src
cargo build --release --features flutter --lib
cd flutter
flutter build windows --release

# 3. Запустить
.\build\windows\x64\runner\Release\ProbationDesk.exe
```

---

**Готово! Теперь у вас полностью рабочий Probation Desk! 🎉**
