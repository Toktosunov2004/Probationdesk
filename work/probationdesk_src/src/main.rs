#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use librustdesk::*;

// -------------------- Flutter / мобильные сборки --------------------
#[cfg(any(target_os = "android", target_os = "ios", feature = "flutter"))]
fn main() {
    if !common::global_init() {
        eprintln!("Global initialization failed.");
        return;
    }

    // Лёгкая самопроверка сети/сервера — как в оригинале.
    common::test_rendezvous_server();
    common::test_nat_type();

    common::global_clean();
}

// -------------------- Настольная GUI сборка (по умолчанию) --------------------
#[cfg(not(any(
    target_os = "android",
    target_os = "ios",
    feature = "cli",
    feature = "flutter"
)))]
fn main() {
    if !common::global_init() {
        return;
    }

    // Включаем Per-Monitor DPI Awareness только если НЕ задана фича `no-dpi`.
    #[cfg(all(windows, not(feature = "no-dpi")))]
    unsafe {
        use winapi::um::shellscalingapi::SetProcessDpiAwareness;
        // 2 == PROCESS_PER_MONITOR_DPI_AWARE
        SetProcessDpiAwareness(2);
    }

    // Запускаем основной UI
    if let Some(args) = crate::core_main::core_main().as_mut() {
        ui::start(args);
    }

    common::global_clean();
}

// -------------------- CLI сборка --------------------
#[cfg(feature = "cli")]
fn main() {
    // Вся логика CLI — в отдельном модуле.
    // Это важно для разделения зависимостей и чтобы GUI код не тянулся в CLI.
    cli::main_cli();
}
