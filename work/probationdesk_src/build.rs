// build.rs — ресурсы Windows + автокопирование sciter.dll

use std::{env, fs, path::PathBuf};

#[cfg(target_os = "windows")]
fn embed_icon() {
    // Если winres отсутствует в build-dependencies — добавьте в Cargo.toml:
    // [target.'cfg(target_os="windows")'.build-dependencies]
    // winres = "0.1"
    let mut res = winres::WindowsResource::new();
    // Укажите свой .ico
    res.set_icon("res/probationdesk.ico");
    if let Err(e) = res.compile() {
        // Не валим сборку: просто предупредим
        println!("cargo:warning=winres compile failed: {e}");
    }
}

#[cfg(target_os = "windows")]
fn maybe_copy_sciter() {
    // Профиль сборки: "debug" или "release"
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Каталог target: honor CARGO_TARGET_DIR, иначе ./target
    let target_dir = PathBuf::from(
        env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string())
    );

    let dest = target_dir.join(&profile).join("sciter.dll");
    if dest.exists() {
        // Уже есть — ничего не делаем.
        return;
    }

    // Кандидаты-источники sciter.dll:
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1) SCITER_SDK/bin/64/sciter.dll
    if let Ok(sdk) = env::var("SCITER_SDK") {
        candidates.push(PathBuf::from(sdk).join("bin").join("64").join("sciter.dll"));
    }

    // 2) ./sciter/bin/64/sciter.dll (если рядом с проектом держите SDK)
    candidates.push(PathBuf::from("sciter").join("bin").join("64").join("sciter.dll"));

    // 3) ./res/sciter.dll (если покладёте рядом)
    candidates.push(PathBuf::from("res").join("sciter.dll"));

    // Ищем первый существующий и копируем
    let found = candidates.iter().find(|p| p.exists()).cloned();

    match found {
        Some(src) => {
            if let Err(e) = fs::create_dir_all(dest.parent().unwrap()) {
                println!("cargo:warning=failed to create target dir for sciter.dll: {e}");
                return;
            }
            match fs::copy(&src, &dest) {
                Ok(_) => {
                    println!("cargo:warning=Copied sciter.dll from '{}' to '{}'",
                        src.display(), dest.display());
                }
                Err(e) => {
                    println!("cargo:warning=Failed to copy sciter.dll: {e}");
                }
            }
        }
        None => {
            println!(
                "cargo:warning=sciter.dll not found. Set SCITER_SDK or place sciter.dll to {:?} or {:?}",
                PathBuf::from("sciter").join("bin").join("64").join("sciter.dll"),
                PathBuf::from("res").join("sciter.dll")
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn main() {
    // Чтобы пересобиралось при изменении иконки/пути SDK:
    println!("cargo:rerun-if-changed=res/probationdesk.ico");
    println!("cargo:rerun-if-env-changed=SCITER_SDK");

    embed_icon();
    maybe_copy_sciter();
}

#[cfg(not(target_os = "windows"))]
fn main() {
    // Ничего не делаем на других платформах.
}
