//! Windows 専用部分: ホットキーの登録・待ち受け・実行。
//! `hotrun`（コンソール）と `hotrun-app`（設定画面・トレイ常駐）の両方から使う。

use crate::{Config, Entry, Window};
use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_CLOSE_EVENT, CTRL_C_EVENT};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, SHOW_WINDOW_CMD, SW_HIDE, SW_SHOWMAXIMIZED,
    SW_SHOWMINNOACTIVE, SW_SHOWNORMAL, WM_HOTKEY, WM_QUIT,
};

/// ERROR_HOTKEY_ALREADY_REGISTERED
const ALREADY_REGISTERED: u32 = 1409;

/// 取れなかったキー（項目の id, キー名, 理由）
pub type Failure = (String, String, String);

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 空文字なら NULL を渡す
fn opt(buf: &Option<Vec<u16>>) -> PCWSTR {
    buf.as_ref().map_or(PCWSTR::null(), |b| PCWSTR(b.as_ptr()))
}

/// ShellExecute の "open" で開く（フォルダ・ファイル・プログラムのどれでも関連付けに従う）
pub fn shell_open(target: &str, args: &str, cwd: &str, show: SHOW_WINDOW_CMD) -> Result<(), String> {
    let target = wide(target);
    let args = (!args.trim().is_empty()).then(|| wide(args));
    let cwd = (!cwd.trim().is_empty()).then(|| wide(cwd));
    let verb = wide("open");
    let r = unsafe { ShellExecuteW(HWND::default(), PCWSTR(verb.as_ptr()), PCWSTR(target.as_ptr()), opt(&args), opt(&cwd), show) };
    // 32 以下ならエラー
    let code = r.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(shell_error(code))
    }
}

/// 1 項目を実行する
pub fn execute(e: &Entry) -> Result<(), String> {
    let show = match e.window {
        Window::Normal => SW_SHOWNORMAL,
        Window::Maximized => SW_SHOWMAXIMIZED,
        Window::Minimized => SW_SHOWMINNOACTIVE,
        Window::Hidden => SW_HIDE,
    };
    shell_open(&e.target, &e.args, &e.cwd, show)
}

fn shell_error(code: isize) -> String {
    match code {
        0 | 8 => "メモリ不足".into(),
        2 => "ファイルが見つかりません".into(),
        3 => "パスが見つかりません".into(),
        5 => "アクセスが拒否されました".into(),
        26 => "共有違反".into(),
        27 | 31 => "関連付けられたアプリがありません".into(),
        28..=30 => "DDE の処理に失敗しました".into(),
        32 => "DLL が見つかりません".into(),
        n => format!("ShellExecute がエラー {n} を返しました"),
    }
}

fn register(id: i32, e: &Entry) -> Result<(), String> {
    let mods = HOT_KEY_MODIFIERS(e.hotkey.mods) | MOD_NOREPEAT;
    unsafe { RegisterHotKey(HWND::default(), id, mods, e.hotkey.vk) }.map_err(|err| {
        if err.code() == windows::core::HRESULT::from_win32(ALREADY_REGISTERED) {
            "他のアプリがこのキーを使用中です（どのアプリかは Windows から分かりません）".to_string()
        } else {
            format!("登録できません: {}", err.message().trim())
        }
    })
}

/// 有効な項目を全部登録する。登録できた id と、取れなかった項目を返す。
/// WM_HOTKEY の wParam は、項目の並び順 + 1。
pub fn register_all(cfg: &Config) -> (Vec<i32>, Vec<Failure>) {
    let mut ok = vec![];
    let mut failed = vec![];
    for (i, e) in cfg.entries.iter().enumerate().filter(|(_, e)| e.enabled) {
        let id = i as i32 + 1;
        match register(id, e) {
            Ok(()) => ok.push(id),
            Err(why) => failed.push((e.id.clone(), e.hotkey.to_string(), why)),
        }
    }
    (ok, failed)
}

pub fn unregister_all(ids: &[i32]) {
    for &id in ids {
        unsafe {
            let _ = UnregisterHotKey(HWND::default(), id);
        }
    }
}

/// WM_HOTKEY の wParam から項目を引く
pub fn entry_for(cfg: &Config, id: i32) -> Option<&Entry> {
    usize::try_from(id - 1).ok().and_then(|i| cfg.entries.get(i))
}

/// 登録できるかだけを調べる（すぐに解除する）。失敗した項目を返す。
pub fn probe(cfg: &Config) -> Vec<Failure> {
    let (ok, failed) = register_all(cfg);
    unregister_all(&ok);
    failed
}

static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn on_ctrl(kind: u32) -> BOOL {
    if kind == CTRL_C_EVENT || kind == CTRL_CLOSE_EVENT {
        let tid = MAIN_THREAD.load(Ordering::SeqCst);
        if tid != 0 {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        return BOOL(1);
    }
    BOOL(0)
}

/// 登録して待ち受ける。Ctrl+C で終了。
pub fn run(cfg: &Config) -> Result<(), String> {
    let (registered, failed) = register_all(cfg);
    for (id, key, why) in &failed {
        eprintln!("取れませんでした: {id} {key} … {why}");
    }
    println!("{} 件を登録（取れなかったもの {} 件）。Ctrl+C で終了します。", registered.len(), failed.len());

    MAIN_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(on_ctrl), true);
    }

    let mut msg = MSG::default();
    loop {
        let r = unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) };
        if r.0 <= 0 {
            break; // 0 = WM_QUIT、-1 = エラー
        }
        if msg.message == WM_HOTKEY {
            if let Some(e) = entry_for(cfg, msg.wParam.0 as i32) {
                match execute(e) {
                    Ok(()) => println!("実行: {} {}", e.hotkey, e.target),
                    Err(why) => eprintln!("失敗: {} {} … {}", e.hotkey, e.target, why),
                }
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    unregister_all(&registered);
    Ok(())
}

// ---- 自動起動（HKCU\...\Run） ----

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

pub fn autostart_enabled(app_name: &str) -> bool {
    let key = wide(RUN_KEY);
    let name = wide(app_name);
    let mut hkey = HKEY::default();
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), 0, KEY_READ, &mut hkey) != ERROR_SUCCESS {
            return false;
        }
        let found = RegQueryValueExW(hkey, PCWSTR(name.as_ptr()), None, None, None, None) == ERROR_SUCCESS;
        let _ = RegCloseKey(hkey);
        found
    }
}

/// 自動起動を設定／解除する。`enable` が Some なら、その文字列（コマンドライン全体）を Run キーに書く。None なら解除。
pub fn set_autostart(app_name: &str, enable: Option<&str>) -> Result<(), String> {
    let key = wide(RUN_KEY);
    let name = wide(app_name);
    let mut hkey = HKEY::default();
    unsafe {
        let r = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if r != ERROR_SUCCESS {
            return Err(format!("レジストリを開けません（エラー {}）", r.0));
        }
        let result = if let Some(cmd) = enable {
            let v = wide(cmd);
            let bytes = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
            let r = RegSetValueExW(hkey, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes));
            if r == ERROR_SUCCESS { Ok(()) } else { Err(format!("書き込めません（エラー {}）", r.0)) }
        } else {
            let r = RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
            if r == ERROR_SUCCESS || r.0 == 2 { Ok(()) } else { Err(format!("削除できません（エラー {}）", r.0)) }
        };
        let _ = RegCloseKey(hkey);
        result
    }
}
