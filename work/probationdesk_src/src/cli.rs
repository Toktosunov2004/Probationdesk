// src/cli.rs — минимальный, безопасно компилируемый CLI-stub

use std::sync::{Arc, RwLock};

use clap::{Arg, ArgAction, Command};
use hbb_common::log;

// ТИПЫ: берем WindowsSession напрямую из message_proto (а не из crate::client),
// иначе ловим "private struct import".
use hbb_common::{
    message_proto::{Data, Hash, PeerInfo, TestDelay, WindowsSession},
    Stream,
};

// Трейт Interface и LoginConfigHandler — из клиента.
// (Путь может отличаться в вашей ветке; если компилятор ругнётся — поправьте.)
use crate::client::{Interface, LoginConfigHandler};

/// Простейшая "сессия" только ради соответствия трейту Interface.
#[derive(Clone)]
pub struct Session {
    lch: Arc<RwLock<LoginConfigHandler>>,
}

impl Session {
    pub fn new(lch: Arc<RwLock<LoginConfigHandler>>) -> Self {
        Self { lch }
    }
}

// === Реализация обязательных методов трейта Interface ===
// Трейт у вас требует Send + Clone + 'static + Sized уже выполнено (Arc<RwLock<...>> + #[derive(Clone)]).
impl Interface for Session {
    fn set_multiple_windows_session(&self, _sessions: Vec<WindowsSession>) {
        // stub: ничего не делаем
    }

    fn get_lch(&self) -> Arc<RwLock<LoginConfigHandler>> {
        self.lch.clone()
    }

    fn send(&self, _data: Data) {
        // stub: заглушка
    }

    fn msgbox(&self, msgtype: &str, title: &str, text: &str, _link: &str) {
        // stub: просто залогируем
        match msgtype {
            m if m.contains("error") => log::error!("{}: {}", title, text),
            _ => log::info!("{}: {}", title, text),
        }
    }

    fn handle_login_error(&self, err: &str) -> bool {
        // stub: просто лог и сказать "не обработано"
        log::warn!("handle_login_error: {}", err);
        false
    }

    fn handle_peer_info(&self, _pi: PeerInfo) {
        // stub
    }

    // В интерфейсе объявлены async-методы. Их можно реализовать как обычные
    // (без async) ТОЛЬКО если сам трейт помечен #[async_trait]. У вас он уже
    // так объявлен, поэтому просто реализуем сигнатуры как у трейта.
    // Если вдруг компилятор попросит #[async_trait] здесь — добавьте и в этот impl.
    fn handle_hash<'life0, 'async_trait>(
        &'life0 self,
        _pass: &'life0 str,
        _hash: Hash,
        _peer: &'life0 mut Stream,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ()> + Send + 'async_trait>
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            // stub
        })
    }

    fn handle_login_from_ui<'life0, 'async_trait>(
        &'life0 self,
        _os_username: String,
        _os_password: String,
        _password: String,
        _remember: bool,
        _peer: &'life0 mut Stream,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ()> + Send + 'async_trait>
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            // stub
        })
    }

    fn handle_test_delay<'life0, 'async_trait>(
        &'life0 self,
        _t: TestDelay,
        _peer: &'life0 mut Stream,
    ) -> core::pin::Pin<
        Box<dyn core::future::Future<Output = ()> + Send + 'async_trait>
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            // stub
        })
    }
}

pub fn cli_entry() -> i32 {
    let matches = Command::new("rustdesk")
        .about("RustDesk command line tool (minimal CLI)")
        .version(crate::VERSION)
        .arg(
            Arg::new("port-forward")
                .short('p')
                .long("port-forward")
                .num_args(1)
                .help("Format: remote-id:local-port:remote-port[:remote-host]"),
        )
        .arg(
            Arg::new("connect")
                .short('c')
                .long("connect")
                .num_args(1)
                .help("Test connect to remote-id"),
        )
        .arg(
            Arg::new("key")
                .short('k')
                .long("key")
                .num_args(1)
                .help("Auth key/token"),
        )
        .arg(
            Arg::new("server")
                .short('s')
                .long("server")
                .action(ArgAction::SetTrue)
                .help("Start server"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .action(ArgAction::Count)
                .help("Increase verbosity (-v, -vv)"),
        )
        .get_matches();

    // Логгер
    {
        use hbb_common::env_logger::*;
        let default_level = match matches.get_count("verbose") {
            0 => "info",
            1 => "debug",
            _ => "trace",
        };
        init_from_env(Env::default().filter_or(DEFAULT_FILTER_ENV, default_level));
    }

    if let Some(pf) = matches.get_one::<String>("port-forward") {
        log::info!("Requested port-forward: {}", pf);
        log::warn!("Минимальный CLI-stub: функционал port-forward не активирован в этой сборке.");
        return 0;
    }

    if let Some(cid) = matches.get_one::<String>("connect") {
        let _key = matches.get_one::<String>("key").cloned().unwrap_or_default();
        log::info!("Requested connect to id: {}", cid);
        log::warn!("Минимальный CLI-stub: функционал connect не активирован в этой сборке.");
        return 0;
    }

    if matches.get_flag("server") {
        log::info!("Server mode requested");
        log::warn!("Минимальный CLI-stub: запуск сервера отключён в этой сборке.");
        return 0;
    }

    // Если флаги не переданы — вывести help.
    println!("{}", Command::new("rustdesk").render_long_help());
    0
}

// Точка входа под feature = "cli"
#[cfg(feature = "cli")]
pub fn main_cli() {
    if !crate::common::global_init() {
        eprintln!("Global initialization failed.");
        return;
    }

    // Заглушка LoginConfigHandler для соответствия трейту
    let lch = Arc::new(RwLock::new(LoginConfigHandler::default()));
    let _session = Session::new(lch);

    let code = cli_entry();
    if code != 0 {
        log::error!("CLI exited with code {}", code);
    }

    crate::common::global_clean();
}
