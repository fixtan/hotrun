//! HKLaunch の CSV 取り込み。
//!
//! 列: 修飾キー, 仮想キー, 種類, コマンド, 引数, 作業ディレクトリ, ウィンドウ, メニュー文字列, フラグ, グループ
//! 種類（0=実行 / 1=フォルダ / 2=ファイル）とウィンドウ値は実データからの推定で、
//! 確認できていないものは警告に出す。

use crate::config::{Config, Entry, Kind, Window};
use crate::hotkey::Hotkey;

pub struct Imported {
    pub config: Config,
    pub warnings: Vec<String>,
}

/// UTF-8 として正しければそのまま、そうでなければ Shift-JIS として読む
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
    }
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_q = !in_q,
            ',' if !in_q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).find(|s| !s.is_empty()).unwrap_or(path)
}

pub fn import_hklaunch(bytes: &[u8]) -> Imported {
    let text = decode(bytes);
    let mut entries = vec![];
    let mut warnings = vec![];

    for (i, line) in text.lines().enumerate() {
        let line_no = i + 1;
        if line.trim().is_empty() {
            continue;
        }
        let f = split_csv_line(line);
        if f.len() < 8 {
            warnings.push(format!("{line_no} 行目: 列が足りないので飛ばしました"));
            continue;
        }
        let (Ok(mods), Ok(vk), Ok(ty)) = (f[0].trim().parse::<u32>(), f[1].trim().parse::<u32>(), f[2].trim().parse::<u32>())
        else {
            warnings.push(format!("{line_no} 行目: 数値を読めないので飛ばしました"));
            continue;
        };
        let kind = match ty {
            0 => Kind::Run,
            1 => Kind::OpenFolder,
            2 => Kind::OpenFile,
            n => {
                warnings.push(format!("{line_no} 行目: 種類 {n} は未確認なのでファイルとして扱いました"));
                Kind::OpenFile
            }
        };
        let window = match f[6].trim().parse::<u32>().unwrap_or(1) {
            0 => Window::Hidden,
            3 => Window::Maximized,
            2 | 6 | 7 => Window::Minimized,
            _ => Window::Normal,
        };
        let target = f[3].trim().to_string();
        let label = {
            let l = f[7].trim();
            if l.is_empty() || l == target || l == file_name(&target) { String::new() } else { l.to_string() }
        };
        entries.push(Entry {
            id: format!("e{:02}", entries.len() + 1),
            hotkey: Hotkey { mods: mods & 0xF, vk },
            kind,
            target,
            args: f[4].trim().to_string(),
            cwd: f[5].trim().to_string(),
            window,
            label,
            group: f.get(9).map(|s| s.trim().to_string()).unwrap_or_default(),
            enabled: true,
        });
    }
    Imported { config: Config { version: 1, entries }, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "6,49,1,\"\\\\192.168.0.1\\smb\\\",\"\",\"\",5,\"\\\\192.168.0.1\\smb\\\",67109424,\"\"\r\n\
2,49,2,\"D:\\[DATA]\\[自プログラミング]\\PuTTY.exe\",\"\",\"\",5,\"PuTTY.exe\",67109424,\"\"\r\n\
0,130,0,\"D:\\tools\\モニタ電源オフ.exe\",\"\",\"D:\\tools\",5,\"モニタ電源オフ\",67109424,\"\"\r\n\
\r\n";

    #[test]
    fn imports_shift_jis_csv() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(SAMPLE);
        let r = import_hklaunch(&bytes);
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let e = &r.config.entries;
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].hotkey.to_string(), "Ctrl+Shift+1");
        assert_eq!(e[0].kind, Kind::OpenFolder);
        assert_eq!(e[0].target, "\\\\192.168.0.1\\smb\\");
        assert_eq!(e[0].label, "");
        assert_eq!(e[1].hotkey.to_string(), "Ctrl+1");
        assert_eq!(e[1].target, "D:\\[DATA]\\[自プログラミング]\\PuTTY.exe");
        assert_eq!(e[2].hotkey.to_string(), "F19");
        assert_eq!(e[2].kind, Kind::Run);
        assert_eq!(e[2].cwd, "D:\\tools");
        assert_eq!(e[2].label, "モニタ電源オフ");
    }

    #[test]
    fn utf8_input_also_works() {
        let r = import_hklaunch(SAMPLE.as_bytes());
        assert_eq!(r.config.entries.len(), 3);
    }

    #[test]
    fn bad_lines_become_warnings() {
        let r = import_hklaunch(b"x,y\nfoo,bar,baz,qux,a,b,c,d\n");
        assert!(r.config.entries.is_empty());
        assert_eq!(r.warnings.len(), 2);
    }
}
