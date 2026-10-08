//! Tauri 側: 一覧ウィンドウ、編集ウィンドウ、トレイ、コマンド。

use crate::engine::{Engine, Failure, Status};
use crate::store::{display_name, EntryInput, Store};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

const APP_NAME: &str = "HotRun";
const TRAY_ID: &str = "hotrun-tray";

pub struct AppState {
    store: Mutex<Store>,
    engine: Engine,
    status: Mutex<Status>,
    /// 編集ウィンドウのラベル → 編集中の項目 id（新規は空文字）
    edits: Mutex<HashMap<String, String>>,
    seq: AtomicU32,
    /// --config で指定された場合（自動起動のコマンドにも引き継ぐ）
    custom_config: Option<PathBuf>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---- 画面へ渡す形 ----

#[derive(Serialize)]
struct EntryView {
    #[serde(flatten)]
    input: EntryInput,
    name: String,
}

#[derive(Serialize)]
struct StateDto {
    path: String,
    load_error: Option<String>,
    entries: Vec<EntryView>,
    registered: usize,
    failed: Vec<Failure>,
    issues: Vec<String>,
    autostart: bool,
}

#[derive(Serialize)]
struct EditDto {
    entry: EntryInput,
    is_new: bool,
    failure: Option<String>,
}

#[derive(Serialize)]
struct SaveResult {
    id: String,
    /// 保存はできたが、ホットキーが取れなかった場合の理由
    warning: Option<String>,
}

// ---- 共通処理 ----

fn tooltip(status: &Status) -> String {
    if status.failed.is_empty() {
        format!("{APP_NAME}（{} 件）", status.registered)
    } else {
        format!("{APP_NAME}（{} 件、取れなかったキー {} 件）", status.registered, status.failed.len())
    }
}

/// 設定をホットキーへ反映し、画面とトレイを更新する
fn apply(app: &AppHandle, st: &AppState) -> Status {
    let cfg = lock(&st.store).config().clone();
    let status = st.engine.reload(cfg);
    *lock(&st.status) = status.clone();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip(&status)));
    }
    let _ = app.emit("config-changed", ());
    status
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn autostart_command(custom: Option<&PathBuf>) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let mut cmd = format!("\"{}\" --hidden", exe.display());
    if let Some(p) = custom {
        cmd.push_str(&format!(" --config \"{}\"", p.display()));
    }
    Some(cmd)
}

fn failure_of(st: &AppState, id: &str) -> Option<String> {
    lock(&st.status).failed.iter().find(|f| f.id == id).map(|f| f.why.clone())
}

// ---- コマンド ----

#[tauri::command]
fn get_state(st: State<'_, AppState>) -> StateDto {
    let store = lock(&st.store);
    let status = lock(&st.status).clone();
    StateDto {
        path: store.path().display().to_string(),
        load_error: store.error().map(String::from),
        entries: store
            .config()
            .entries
            .iter()
            .map(|e| EntryView { input: EntryInput::from_entry(e), name: display_name(e) })
            .collect(),
        registered: status.registered,
        failed: status.failed,
        issues: store.config().check().iter().map(|i| i.to_string()).collect(),
        autostart: autostart_enabled(),
    }
}

/// 編集ウィンドウを開く。id が空なら新規。同じ項目がすでに開いていればそれを前面に出す。
#[tauri::command]
async fn open_edit(app: AppHandle, st: State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let edits = lock(&st.edits);
        if !id.is_empty() {
            if let Some((label, _)) = edits.iter().find(|(_, v)| **v == id) {
                if let Some(w) = app.get_webview_window(label) {
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                    return Ok(());
                }
            }
        }
    }
    let label = format!("edit-{}", st.seq.fetch_add(1, Ordering::SeqCst));
    lock(&st.edits).insert(label.clone(), id.clone());
    let title = if id.is_empty() { format!("{APP_NAME} - 追加") } else { format!("{APP_NAME} - 編集") };
    let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("edit.html".into()))
        .title(title)
        .inner_size(560.0, 640.0)
        .min_inner_size(460.0, 480.0)
        .center()
        .build();
    if let Err(e) = built {
        lock(&st.edits).remove(&label);
        return Err(format!("ウィンドウを開けません: {e}"));
    }
    Ok(())
}

#[tauri::command]
fn edit_target(window: WebviewWindow, st: State<'_, AppState>) -> Result<EditDto, String> {
    let id = lock(&st.edits).get(window.label()).cloned().ok_or("編集対象が不明です")?;
    if id.is_empty() {
        let entry = EntryInput { enabled: true, ..Default::default() };
        return Ok(EditDto { entry, is_new: true, failure: None });
    }
    let store = lock(&st.store);
    let e = store.get(&id).ok_or("この項目はすでに削除されています")?;
    Ok(EditDto { entry: EntryInput::from_entry(e), is_new: false, failure: failure_of(&st, &id) })
}

#[tauri::command]
fn save_entry(app: AppHandle, window: WebviewWindow, st: State<'_, AppState>, input: EntryInput) -> Result<SaveResult, String> {
    let id = lock(&st.store).upsert(&input)?;
    lock(&st.edits).insert(window.label().to_string(), id.clone());
    apply(&app, &st);
    let warning = failure_of(&st, &id);
    if warning.is_none() {
        let _ = window.close();
    }
    Ok(SaveResult { id, warning })
}

#[tauri::command]
fn delete_entry(app: AppHandle, st: State<'_, AppState>, id: String) -> Result<(), String> {
    lock(&st.store).delete(&id)?;
    // 開いている編集ウィンドウは閉じる
    let labels: Vec<String> = lock(&st.edits).iter().filter(|(_, v)| **v == id).map(|(k, _)| k.clone()).collect();
    for l in labels {
        if let Some(w) = app.get_webview_window(&l) {
            let _ = w.close();
        }
    }
    apply(&app, &st);
    Ok(())
}

#[tauri::command]
fn set_enabled(app: AppHandle, st: State<'_, AppState>, id: String, enabled: bool) -> Result<(), String> {
    lock(&st.store).set_enabled(&id, enabled)?;
    apply(&app, &st);
    Ok(())
}

/// 設定ファイルを外部で編集したあとの読み直し
#[tauri::command]
fn reload_config(app: AppHandle, st: State<'_, AppState>) -> Result<(), String> {
    let r = lock(&st.store).reread();
    apply(&app, &st);
    r
}

#[tauri::command]
fn open_config_file(st: State<'_, AppState>) -> Result<(), String> {
    let path = lock(&st.store).path().display().to_string();
    open_in_editor(&path)
}

#[tauri::command]
fn set_autostart(st: State<'_, AppState>, enable: bool) -> Result<(), String> {
    set_autostart_impl(enable, st.custom_config.as_ref())
}

/// ファイルまたはフォルダを選ぶ。キャンセルなら None。
#[tauri::command]
async fn pick_path(app: AppHandle, folder: bool, current: String) -> Result<Option<String>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut d = app.dialog().file();
        d = d.set_title(if folder { "フォルダを選択" } else { "ファイルを選択" });
        let start = std::path::Path::new(&current);
        if let Some(dir) = if start.is_dir() { Some(start) } else { start.parent().filter(|p| p.is_dir()) } {
            d = d.set_directory(dir);
        }
        if folder {
            d.blocking_pick_folder()
        } else {
            d.blocking_pick_file()
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    match picked {
        Some(p) => Ok(Some(p.into_path().map_err(|e| e.to_string())?.display().to_string())),
        None => Ok(None),
    }
}

/// 編集中の内容でその場で実行してみる（保存はしない）
#[tauri::command]
fn test_entry(input: EntryInput) -> Result<(), String> {
    let entry = input.to_entry("test".into())?;
    execute(&entry)
}

/// 他のアプリが使っているホットキーを洗い出す（キーは押さない）。HotRun が取れているキーは「HotRun」に分ける。
#[tauri::command]
async fn scan_hotkeys(st: State<'_, AppState>) -> Result<Vec<hotrun_core::scan::Taken>, String> {
    let own: Vec<hotrun_core::Hotkey> = {
        let store = lock(&st.store);
        let failed = lock(&st.status).failed.clone();
        store
            .config()
            .entries
            .iter()
            .filter(|e| e.enabled && !failed.iter().any(|f| f.id == e.id))
            .map(|e| e.hotkey)
            .collect()
    };
    // RegisterHotKey は呼んだスレッドに紐づくので、専用のスレッドで走らせる
    tauri::async_runtime::spawn_blocking(move || scan_impl(&own)).await.map_err(|e| e.to_string())?
}

/// 使用中キーの一覧ウィンドウを開く（開いていれば前面に出す）
#[tauri::command]
async fn open_scan(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("scan") {
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "scan", WebviewUrl::App("scan.html".into()))
        .title(format!("{APP_NAME} - 使用中のキー一覧"))
        .inner_size(640.0, 640.0)
        .min_inner_size(480.0, 360.0)
        .center()
        .build()
        .map(|_| ())
        .map_err(|e| format!("ウィンドウを開けません: {e}"))
}

#[tauri::command]
fn close_self(window: WebviewWindow) {
    let _ = window.close();
}

// ---- Windows 依存の部分（Windows 以外ではエラーにするだけ） ----

#[cfg(windows)]
fn execute(e: &hotrun_core::Entry) -> Result<(), String> {
    hotrun_core::win::execute(e)
}
#[cfg(not(windows))]
fn execute(_: &hotrun_core::Entry) -> Result<(), String> {
    Err("実行は Windows でのみ使えます".into())
}

#[cfg(windows)]
fn scan_impl(own: &[hotrun_core::Hotkey]) -> Result<Vec<hotrun_core::scan::Taken>, String> {
    let taken = hotrun_core::win::scan_taken(&hotrun_core::scan::candidates());
    Ok(hotrun_core::scan::build(&taken, own))
}
#[cfg(not(windows))]
fn scan_impl(_: &[hotrun_core::Hotkey]) -> Result<Vec<hotrun_core::scan::Taken>, String> {
    Err("Windows でのみ使えます".into())
}

#[cfg(windows)]
fn open_in_editor(path: &str) -> Result<(), String> {
    use hotrun_core::win::shell_open;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    if shell_open(path, "", "", SW_SHOWNORMAL).is_ok() {
        return Ok(());
    }
    // JSON に関連付けがなければ、メモ帳で開く
    shell_open("notepad.exe", &format!("\"{path}\""), "", SW_SHOWNORMAL).map_err(|e| format!("設定ファイルを開けません: {e}"))
}
#[cfg(not(windows))]
fn open_in_editor(_: &str) -> Result<(), String> {
    Err("Windows でのみ使えます".into())
}

#[cfg(windows)]
fn autostart_enabled() -> bool {
    hotrun_core::win::autostart_enabled(APP_NAME)
}
#[cfg(not(windows))]
fn autostart_enabled() -> bool {
    false
}

#[cfg(windows)]
fn set_autostart_impl(enable: bool, custom: Option<&PathBuf>) -> Result<(), String> {
    if enable {
        let cmd = autostart_command(custom).ok_or("実行ファイルの場所を取得できません")?;
        hotrun_core::win::set_autostart(APP_NAME, Some(&cmd))
    } else {
        hotrun_core::win::set_autostart(APP_NAME, None)
    }
}
#[cfg(not(windows))]
fn set_autostart_impl(_: bool, custom: Option<&PathBuf>) -> Result<(), String> {
    let _ = autostart_command(custom);
    Err("Windows でのみ使えます".into())
}

// ---- 起動 ----

fn arg_value(args: &[String], names: &[&str]) -> Option<String> {
    let i = args.iter().position(|a| names.contains(&a.as_str()))?;
    args.get(i + 1).cloned()
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "設定を開く", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "設定ファイルを再読み込み", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &reload, &sep, &quit])?;

    let mut b = TrayIconBuilder::with_id(TRAY_ID).tooltip(APP_NAME).menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.on_menu_event(|app, event| match event.id().as_ref() {
        "open" => show_main(app),
        "reload" => {
            let st = app.state::<AppState>();
            let r = lock(&st.store).reread();
            let status = apply(app, &st);
            match r {
                Err(e) => {
                    app.dialog().message(format!("設定を読み込めませんでした。\n\n{e}")).title(APP_NAME).kind(MessageDialogKind::Error).show(|_| {});
                }
                Ok(()) if !status.failed.is_empty() => show_main(app),
                Ok(()) => {}
            }
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            show_main(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hidden = args.iter().any(|a| a == "--hidden");
    let custom_config = arg_value(&args, &["--config", "-c"]).map(PathBuf::from);

    let result = tauri::Builder::default()
        // 二重起動したら、いまの画面を前面に出す
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            open_edit,
            edit_target,
            save_entry,
            delete_entry,
            set_enabled,
            reload_config,
            open_config_file,
            set_autostart,
            pick_path,
            test_entry,
            scan_hotkeys,
            open_scan,
            close_self,
        ])
        .on_window_event(|window, event| match event {
            // 一覧ウィンドウは閉じてもトレイに残る
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Destroyed if window.label().starts_with("edit-") => {
                if let Some(st) = window.app_handle().try_state::<AppState>() {
                    lock(&st.edits).remove(window.label());
                }
            }
            _ => {}
        })
        .setup(move |app| {
            let path = custom_config.clone().unwrap_or_else(hotrun_core::config::default_path);
            let store = Store::open(path);
            let handle = app.handle().clone();
            let (engine, status) = Engine::start(store.config().clone(), move |msg| {
                handle.dialog().message(msg).title(APP_NAME).kind(MessageDialogKind::Error).show(|_| {});
            });
            app.manage(AppState {
                store: Mutex::new(store),
                engine,
                status: Mutex::new(status.clone()),
                edits: Mutex::new(HashMap::new()),
                seq: AtomicU32::new(1),
                custom_config,
            });

            build_tray(app.handle())?;
            if let Some(tray) = app.tray_by_id(TRAY_ID) {
                let _ = tray.set_tooltip(Some(tooltip(&status)));
            }

            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title(APP_NAME)
                .inner_size(900.0, 580.0)
                .min_inner_size(640.0, 360.0)
                .visible(!hidden)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("HotRun を起動できませんでした: {e}");
        std::process::exit(1);
    }
}
