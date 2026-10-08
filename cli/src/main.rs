use hotrun_core::{import, Config};
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(windows)]
use hotrun_core::win;

const USAGE: &str = "hotrun — ホットキーランチャー（設定は JSON）

使い方:
  hotrun list   [--config FILE]            登録内容を一覧表示
  hotrun check  [--config FILE]            重複・予約キーなどの問題を検査
  hotrun scan   [--config FILE]            他のアプリが使っているホットキーを洗い出す（キーは押さない）
  hotrun check  --live [--config FILE]     実際に登録を試して、取れないキーを検出（Windows）
  hotrun import HKLAUNCH.csv [-o FILE]     HKLaunch の CSV を JSON に変換
  hotrun run    [--config FILE]            ホットキーを登録して待ち受け（Windows。Ctrl+C で終了）
  hotrun fire   ID [--config FILE]         指定した項目を今すぐ実行（Windows）

設定ファイル: --config で指定。省略時は %APPDATA%\\hotrun\\config.json（Windows 以外は ~/.config/hotrun/config.json）";

fn default_config_path() -> PathBuf {
    hotrun_core::config::default_path()
}

/// `--config X` / `-o X` のような「フラグ+値」を取り出し、残りを返す
fn take_opt(args: &mut Vec<String>, names: &[&str]) -> Result<Option<String>, String> {
    if let Some(i) = args.iter().position(|a| names.contains(&a.as_str())) {
        if i + 1 >= args.len() {
            return Err(format!("{} には値が必要です", args[i]));
        }
        let v = args.remove(i + 1);
        args.remove(i);
        return Ok(Some(v));
    }
    Ok(None)
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    match args.iter().position(|a| a == name) {
        Some(i) => {
            args.remove(i);
            true
        }
        None => false,
    }
}

fn run() -> Result<u8, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        println!("{USAGE}");
        return Ok(0);
    }
    let cmd = args.remove(0);
    let cfg_path = take_opt(&mut args, &["--config", "-c"])?.map(PathBuf::from).unwrap_or_else(default_config_path);
    let out_path = take_opt(&mut args, &["-o", "--out"])?;

    match cmd.as_str() {
        "list" => {
            let cfg = Config::load(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
            for e in &cfg.entries {
                let off = if e.enabled { "" } else { "  (無効)" };
                let label = if e.label.is_empty() { &e.target } else { &e.label };
                println!("{:<22} {:<8} {}{}", e.hotkey.to_string(), e.id, label, off);
            }
            println!("{} 件", cfg.entries.len());
            Ok(0)
        }
        "check" => {
            let live = take_flag(&mut args, "--live");
            let cfg = Config::load(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
            let issues = cfg.check();
            for i in &issues {
                println!("{i}");
            }
            #[allow(unused_mut)] // Windows ビルドでだけ加算される
            let mut live_failed = 0;
            if live {
                #[cfg(windows)]
                for (id, key, why) in win::probe(&cfg) {
                    println!("{id}: {key} を取れませんでした … {why}");
                    live_failed += 1;
                }
                #[cfg(not(windows))]
                return Err("--live は Windows でのみ使えます".into());
            }
            if issues.is_empty() && live_failed == 0 {
                println!("問題なし（{} 件）", cfg.entries.len());
                Ok(0)
            } else {
                Ok(1)
            }
        }
        "import" => {
            let src = args.first().ok_or("取り込む CSV のパスを指定してください")?;
            let bytes = std::fs::read(src).map_err(|e| format!("{src}: {e}"))?;
            let r = import::import_hklaunch(&bytes);
            for w in &r.warnings {
                eprintln!("警告: {w}");
            }
            let dest = out_path.map(PathBuf::from).unwrap_or(cfg_path);
            if dest.exists() {
                return Err(format!("{} が既にあります。上書きしないので、-o で別の場所を指定してください", dest.display()));
            }
            r.config.save(&dest).map_err(|e| e.to_string())?;
            println!("{} 件を {} に書き出しました", r.config.entries.len(), dest.display());
            for i in r.config.check() {
                println!("注意: {i}");
            }
            Ok(0)
        }
        "scan" => {
            // 他のアプリが使っているホットキーの洗い出し（キーは押さない）
            #[cfg(windows)]
            {
                // 設定にあるキーは「HotRun のもの」として分ける（hotrun-app 等が動いていればそれが取っている）
                let own: Vec<hotrun_core::Hotkey> = Config::load(&cfg_path)
                    .map(|c| c.entries.iter().filter(|e| e.enabled).map(|e| e.hotkey).collect())
                    .unwrap_or_default();
                let taken = win::scan_taken(&hotrun_core::scan::candidates());
                let list = hotrun_core::scan::build(&taken, &own);
                for (class, title) in [
                    (hotrun_core::scan::Class::HotRun, "HotRun の設定にあるキー"),
                    (hotrun_core::scan::Class::Win, "Win キーを含む組み合わせ（Windows 標準、または PowerToys など）"),
                    (hotrun_core::scan::Class::Other, "他のアプリが使用中（どのアプリかは Windows から分かりません）"),
                ] {
                    let items: Vec<&str> = list.iter().filter(|t| t.class == class).map(|t| t.hotkey.as_str()).collect();
                    println!("\n■ {title}: {} 件", items.len());
                    for k in items {
                        println!("  {k}");
                    }
                }
                println!("\n合計 {} 件", list.len());
                Ok(0)
            }
            #[cfg(not(windows))]
            {
                Err("scan は Windows でのみ使えます".to_string())
            }
        }
        "run" => {
            let cfg = Config::load(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
            #[cfg(windows)]
            {
                win::run(&cfg)?;
                Ok(0)
            }
            #[cfg(not(windows))]
            {
                let _ = cfg;
                Err("run は Windows でのみ使えます".into())
            }
        }
        "fire" => {
            let id = args.first().ok_or("実行する項目の id を指定してください")?.clone();
            let cfg = Config::load(&cfg_path).map_err(|e| format!("{}: {e}", cfg_path.display()))?;
            let entry = cfg.entries.iter().find(|e| e.id == id).ok_or_else(|| format!("id「{id}」の項目がありません"))?;
            #[cfg(windows)]
            {
                win::execute(entry)?;
                Ok(0)
            }
            #[cfg(not(windows))]
            {
                let _ = entry;
                Err("fire は Windows でのみ使えます".into())
            }
        }
        other => Err(format!("不明なコマンド「{other}」\n\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(2)
        }
    }
}
