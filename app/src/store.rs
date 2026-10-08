//! 設定ファイルの編集ロジック（画面に依存しない部分）。
//! 追加・変更・削除はここを通し、検証に通ったものだけをファイルへ書く。

use hotrun_core::{Config, Entry, Hotkey, Issue, Kind, Window};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn yes() -> bool {
    true
}

/// 画面から受け取る 1 項目。ホットキーは文字列（"Ctrl+Shift+1"）のまま受けて、ここで解釈する。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntryInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub hotkey: String,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub window: Window,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub group: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl EntryInput {
    pub fn from_entry(e: &Entry) -> Self {
        EntryInput {
            id: e.id.clone(),
            hotkey: e.hotkey.to_string(),
            kind: e.kind,
            target: e.target.clone(),
            args: e.args.clone(),
            cwd: e.cwd.clone(),
            window: e.window,
            label: e.label.clone(),
            group: e.group.clone(),
            enabled: e.enabled,
        }
    }

    /// 検証つきで Entry にする（id は呼び出し側で決める）
    pub fn to_entry(&self, id: String) -> Result<Entry, String> {
        if self.hotkey.trim().is_empty() {
            return Err("ホットキーを設定してください".into());
        }
        let hotkey = Hotkey::parse(self.hotkey.trim())?;
        let target = self.target.trim().to_string();
        if target.is_empty() {
            return Err("実行するプログラム・ファイル・フォルダを指定してください".into());
        }
        Ok(Entry {
            id,
            hotkey,
            kind: self.kind,
            target,
            args: self.args.trim().to_string(),
            cwd: self.cwd.trim().to_string(),
            window: self.window,
            label: self.label.trim().to_string(),
            group: self.group.trim().to_string(),
            enabled: self.enabled,
        })
    }
}

/// 一覧に出す名前（ラベルがなければ対象のファイル名）
pub fn display_name(e: &Entry) -> String {
    if !e.label.is_empty() {
        return e.label.clone();
    }
    let t = e.target.trim_end_matches(['\\', '/']);
    t.rsplit(['\\', '/']).next().filter(|s| !s.is_empty()).unwrap_or(&e.target).to_string()
}

pub struct Store {
    path: PathBuf,
    cfg: Config,
    /// 読み込みに失敗したときの理由。このあいだは書き込まない（壊れた JSON を上書きしないため）
    error: Option<String>,
}

impl Store {
    /// 読み込む。ファイルがなければ空の設定を作る。JSON が壊れていれば error を持った空の状態になる。
    pub fn open(path: PathBuf) -> Store {
        if !path.exists() {
            let _ = Config::default().save(&path);
        }
        let (cfg, error) = match Config::load(&path) {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(format!("{}\n{e}", path.display()))),
        };
        Store { path, cfg, error }
    }

    /// 外部で編集されたファイルを読み直す。失敗時は今の内容を保つ。
    pub fn reread(&mut self) -> Result<(), String> {
        match Config::load(&self.path) {
            Ok(c) => {
                self.cfg = c;
                self.error = None;
                Ok(())
            }
            Err(e) => {
                let msg = format!("{}\n{e}", self.path.display());
                self.error = Some(msg.clone());
                Err(msg)
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn config(&self) -> &Config {
        &self.cfg
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.cfg.entries.iter().find(|e| e.id == id)
    }

    fn writable(&self) -> Result<(), String> {
        match &self.error {
            Some(_) => Err("設定ファイルを読み込めていないため、変更できません。JSON を直して「再読み込み」してください".into()),
            None => Ok(()),
        }
    }

    pub fn next_id(&self) -> String {
        let max = self
            .cfg
            .entries
            .iter()
            .filter_map(|e| e.id.strip_prefix('e').and_then(|n| n.parse::<u32>().ok()))
            .max()
            .unwrap_or(0);
        format!("e{:02}", max + 1)
    }

    fn commit(&mut self, next: Config) -> Result<(), String> {
        next.save(&self.path).map_err(|e| format!("保存できません: {e}"))?;
        self.cfg = next;
        Ok(())
    }

    /// 追加（id が空）または更新。成功したら項目の id を返す。
    pub fn upsert(&mut self, input: &EntryInput) -> Result<String, String> {
        self.writable()?;
        let is_new = input.id.is_empty();
        let id = if is_new { self.next_id() } else { input.id.clone() };
        if !is_new && self.get(&id).is_none() {
            return Err("この項目はすでに削除されています".into());
        }
        let entry = input.to_entry(id.clone())?;

        let mut next = self.cfg.clone();
        match next.entries.iter_mut().find(|e| e.id == id) {
            Some(slot) => *slot = entry.clone(),
            None => next.entries.push(entry.clone()),
        }
        if entry.enabled {
            self.check_one(&next, &entry)?;
        }
        self.commit(next)?;
        Ok(id)
    }

    /// 今回の項目に関わる問題だけを取り出す（既存の別項目の問題では保存を止めない）
    fn check_one(&self, next: &Config, entry: &Entry) -> Result<(), String> {
        for issue in next.check() {
            match &issue {
                Issue::Reserved { id, .. } if *id == entry.id => return Err(issue.to_string()),
                Issue::BareTypingKey { id, hotkey } if *id == entry.id => {
                    return Err(format!("{hotkey} は修飾キー（Ctrl など）なしの文字キーで、入力を奪ってしまいます"));
                }
                Issue::DuplicateHotkey { hotkey, ids } if ids.contains(&entry.id) => {
                    let other = ids.iter().find(|i| **i != entry.id).and_then(|i| next.entries.iter().find(|e| e.id == *i));
                    return Err(match other {
                        Some(o) => format!("{hotkey} はすでに「{}」に割り当てられています", display_name(o)),
                        None => format!("{hotkey} が重複しています"),
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        self.writable()?;
        if self.get(id).is_none() {
            return Err("この項目はすでに削除されています".into());
        }
        let mut next = self.cfg.clone();
        next.entries.retain(|e| e.id != id);
        self.commit(next)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        let mut input = EntryInput::from_entry(self.get(id).ok_or("この項目はすでに削除されています")?);
        input.enabled = enabled;
        self.upsert(&input).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().join("config.json"));
        (dir, s)
    }

    fn input(key: &str, target: &str) -> EntryInput {
        EntryInput { hotkey: key.into(), target: target.into(), enabled: true, ..Default::default() }
    }

    #[test]
    fn creates_empty_file_when_missing() {
        let (dir, s) = store();
        assert!(dir.path().join("config.json").exists());
        assert!(s.config().entries.is_empty());
        assert!(s.error().is_none());
    }

    #[test]
    fn add_assigns_sequential_ids_and_persists() {
        let (_d, mut s) = store();
        assert_eq!(s.upsert(&input("Ctrl+1", "a.exe")).unwrap(), "e01");
        assert_eq!(s.upsert(&input("Ctrl+2", "b.exe")).unwrap(), "e02");
        let again = Config::load(s.path()).unwrap();
        assert_eq!(again.entries.len(), 2);
        assert_eq!(again.entries[1].target, "b.exe");
    }

    #[test]
    fn next_id_skips_gaps_and_foreign_ids() {
        let (_d, mut s) = store();
        s.upsert(&input("Ctrl+1", "a")).unwrap();
        s.upsert(&input("Ctrl+2", "b")).unwrap();
        s.delete("e01").unwrap();
        assert_eq!(s.next_id(), "e03");
    }

    #[test]
    fn update_replaces_in_place() {
        let (_d, mut s) = store();
        s.upsert(&input("Ctrl+1", "a")).unwrap();
        s.upsert(&input("Ctrl+2", "b")).unwrap();
        let mut edit = EntryInput::from_entry(s.get("e01").unwrap());
        edit.args = "--x".into();
        edit.hotkey = "Ctrl+3".into();
        s.upsert(&edit).unwrap();
        assert_eq!(s.config().entries[0].args, "--x");
        assert_eq!(s.config().entries[0].hotkey.to_string(), "Ctrl+3");
        assert_eq!(s.config().entries.len(), 2);
    }

    #[test]
    fn rejects_duplicate_hotkey_with_name_of_other() {
        let (_d, mut s) = store();
        let mut a = input("Ctrl+1", "C:\\tools\\PuTTY.exe");
        a.label = String::new();
        s.upsert(&a).unwrap();
        let err = s.upsert(&input("ctrl+1", "b")).unwrap_err();
        assert!(err.contains("PuTTY.exe"), "{err}");
        assert_eq!(s.config().entries.len(), 1, "保存されていない");
    }

    #[test]
    fn disabled_entry_may_share_a_key() {
        let (_d, mut s) = store();
        s.upsert(&input("Ctrl+1", "a")).unwrap();
        let mut b = input("Ctrl+1", "b");
        b.enabled = false;
        s.upsert(&b).unwrap();
        // 有効に戻すときに止まる
        assert!(s.set_enabled("e02", true).is_err());
    }

    #[test]
    fn rejects_bad_input() {
        let (_d, mut s) = store();
        assert!(s.upsert(&input("", "a")).unwrap_err().contains("ホットキー"));
        assert!(s.upsert(&input("Ctrl+1", "  ")).unwrap_err().contains("指定"));
        assert!(s.upsert(&input("Ctrl+Foo", "a")).is_err());
        assert!(s.upsert(&input("A", "a")).unwrap_err().contains("修飾キー"));
        assert!(s.upsert(&input("Win+L", "a")).is_err());
        assert!(s.upsert(&input("F19", "a")).is_ok(), "ファンクションキー単独は可");
    }

    #[test]
    fn existing_problems_in_other_entries_do_not_block_saving() {
        let (_d, mut s) = store();
        let text = r#"{"version":1,"entries":[
          {"id":"a","hotkey":"Ctrl+1","target":"x"},
          {"id":"b","hotkey":"Ctrl+1","target":"y"}]}"#;
        std::fs::write(s.path(), text).unwrap();
        s.reread().unwrap();
        s.upsert(&input("Ctrl+5", "z")).unwrap();
        assert_eq!(s.config().entries.len(), 3);
    }

    #[test]
    fn broken_json_is_never_overwritten() {
        let (_d, mut s) = store();
        std::fs::write(s.path(), "{ broken").unwrap();
        assert!(s.reread().is_err());
        assert!(s.error().is_some());
        assert!(s.upsert(&input("Ctrl+1", "a")).is_err());
        assert!(s.delete("e01").is_err());
        assert_eq!(std::fs::read_to_string(s.path()).unwrap(), "{ broken");
        // 直せば復帰する
        std::fs::write(s.path(), r#"{"version":1,"entries":[]}"#).unwrap();
        s.reread().unwrap();
        assert!(s.upsert(&input("Ctrl+1", "a")).is_ok());
    }

    #[test]
    fn update_of_vanished_entry_fails() {
        let (_d, mut s) = store();
        s.upsert(&input("Ctrl+1", "a")).unwrap();
        let mut e = EntryInput::from_entry(s.get("e01").unwrap());
        s.delete("e01").unwrap();
        e.args = "x".into();
        assert!(s.upsert(&e).unwrap_err().contains("削除"));
    }

    #[test]
    fn display_name_prefers_label_then_file_name() {
        let mut i = input("Ctrl+1", "D:\\a\\b\\tool.exe");
        let e = i.to_entry("x".into()).unwrap();
        assert_eq!(display_name(&e), "tool.exe");
        i.target = "\\\\192.168.0.1\\smb\\".into();
        assert_eq!(display_name(&i.to_entry("x".into()).unwrap()), "smb");
        i.label = "メモ".into();
        assert_eq!(display_name(&i.to_entry("x".into()).unwrap()), "メモ");
    }
}
