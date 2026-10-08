//! ホットキーの登録と待ち受け。
//! RegisterHotKey は「登録したスレッド」にメッセージが届くので、専用スレッドで登録とメッセージループを持つ。
//! 画面側からは `reload` で設定を差し替える。

#[cfg(not(windows))]
use hotrun_core::Config;
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Failure {
    pub id: String,
    pub key: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    pub registered: usize,
    pub failed: Vec<Failure>,
}

#[cfg(windows)]
pub use imp::Engine;
#[cfg(not(windows))]
pub use stub::Engine;

#[cfg(windows)]
mod imp {
    use super::{Failure, Status};
    use hotrun_core::{win, Config};
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::time::Duration;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW, TranslateMessage, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY, WM_QUIT,
        WM_USER,
    };

    enum Cmd {
        Reload(Config, Sender<Status>),
    }

    pub struct Engine {
        tid: u32,
        tx: Sender<Cmd>,
    }

    fn to_status(registered: &[i32], failed: Vec<win::Failure>) -> Status {
        Status {
            registered: registered.len(),
            failed: failed.into_iter().map(|(id, key, why)| Failure { id, key, why }).collect(),
        }
    }

    impl Engine {
        /// 専用スレッドを起動して登録する。`on_error` は、ホットキーで起動した先が失敗したときに呼ばれる。
        pub fn start(cfg: Config, on_error: impl Fn(String) + Send + 'static) -> (Engine, Status) {
            let (tx, rx) = channel::<Cmd>();
            let (ready_tx, ready_rx) = channel::<(u32, Status)>();
            std::thread::spawn(move || thread_main(cfg, rx, ready_tx, on_error));
            let (tid, status) = ready_rx.recv().unwrap_or((0, Status::default()));
            (Engine { tid, tx }, status)
        }

        /// 設定を差し替えて登録し直す。登録できなかったキーを返す。
        pub fn reload(&self, cfg: Config) -> Status {
            let (rtx, rrx) = channel();
            if self.tx.send(Cmd::Reload(cfg, rtx)).is_err() {
                return Status::default();
            }
            unsafe {
                let _ = PostThreadMessageW(self.tid, WM_APP, WPARAM(0), LPARAM(0));
            }
            rrx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
        }
    }

    impl Drop for Engine {
        fn drop(&mut self) {
            unsafe {
                let _ = PostThreadMessageW(self.tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }

    fn thread_main(mut cfg: Config, rx: Receiver<Cmd>, ready: Sender<(u32, Status)>, on_error: impl Fn(String)) {
        // メッセージキューを先に作っておく（作る前に PostThreadMessage すると失敗する）
        let mut msg = MSG::default();
        unsafe {
            let _ = PeekMessageW(&mut msg, HWND::default(), WM_USER, WM_USER, PM_NOREMOVE);
        }
        let (mut ids, failed) = win::register_all(&cfg);
        let _ = ready.send((unsafe { GetCurrentThreadId() }, to_status(&ids, failed)));

        loop {
            let r = unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) };
            if r.0 <= 0 {
                break;
            }
            match msg.message {
                WM_HOTKEY => {
                    if let Some(e) = win::entry_for(&cfg, msg.wParam.0 as i32) {
                        if let Err(why) = win::execute(e) {
                            on_error(format!("実行できませんでした\n\n{}  {}\n{}\n\n{why}", e.hotkey, super::name_of(e), e.target));
                        }
                    }
                }
                WM_APP => {
                    while let Ok(Cmd::Reload(next, reply)) = rx.try_recv() {
                        win::unregister_all(&ids);
                        let (new_ids, failed) = win::register_all(&next);
                        ids = new_ids;
                        cfg = next;
                        let _ = reply.send(to_status(&ids, failed));
                    }
                }
                _ => unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                },
            }
        }
        win::unregister_all(&ids);
    }
}

#[cfg(windows)]
fn name_of(e: &hotrun_core::Entry) -> String {
    crate::store::display_name(e)
}

/// Windows 以外では、登録の代わりに件数だけ数える（ロジックのテスト用）
#[cfg(not(windows))]
mod stub {
    use super::{Config, Status};

    pub struct Engine;

    impl Engine {
        pub fn start(cfg: Config, _on_error: impl Fn(String) + Send + 'static) -> (Engine, Status) {
            let s = Engine::count(&cfg);
            (Engine, s)
        }
        pub fn reload(&self, cfg: Config) -> Status {
            Engine::count(&cfg)
        }
        fn count(cfg: &Config) -> Status {
            Status { registered: cfg.entries.iter().filter(|e| e.enabled).count(), failed: vec![] }
        }
    }
}
