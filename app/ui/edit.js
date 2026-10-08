// 編集ウィンドウ（1 項目）
(function () {
  const C = window.HotRunCommon;
  const { invoke } = window.__TAURI__.core;
  const $ = (id) => document.getElementById(id);

  let entry = null; // サーバーから受け取った項目（id や group など、画面に出さない値も保つ）
  let capturing = false;

  function show(id, msg) {
    const el = $(id);
    el.hidden = !msg;
    el.textContent = msg || '';
  }
  function clearMessages() { show('msgError', ''); show('msgWarn', ''); show('msgOk', ''); }

  function read() {
    return {
      ...entry,
      hotkey: $('hotkey').value.trim(),
      kind: $('kind').value,
      target: $('target').value,
      args: $('args').value,
      cwd: $('cwd').value,
      window: $('window').value,
      label: $('label').value,
      enabled: $('enabled').checked,
    };
  }

  function fill(e) {
    $('hotkey').value = e.hotkey || '';
    $('kind').value = e.kind || 'run';
    $('target').value = e.target || '';
    $('args').value = e.args || '';
    $('cwd').value = e.cwd || '';
    $('window').value = e.window || 'normal';
    $('label').value = e.label || '';
    $('enabled').checked = e.enabled !== false;
    applyKind();
  }

  // 動作によって、入力欄の意味と出し分けを変える
  function applyKind() {
    const k = $('kind').value;
    $('targetLabel').textContent = k === 'run' ? 'プログラム' : k === 'open_file' ? 'ファイル' : 'フォルダ';
    $('argsField').hidden = k === 'open_folder';
    $('cwdField').hidden = k === 'open_folder';
    $('windowField').hidden = k === 'open_folder';
  }

  // ---- ホットキーの取り込み ----
  function setCapturing(on) {
    capturing = on;
    $('hotkey').classList.toggle('capturing', on);
    if (on) {
      $('hotkey').dataset.before = $('hotkey').value;
      $('hotkey').value = '';
      $('hotkey').placeholder = 'キーを押してください…';
    } else {
      $('hotkey').placeholder = 'ここをクリックして、割り当てるキーを押す';
    }
  }

  $('hotkey').addEventListener('focus', () => setCapturing(true));
  $('hotkey').addEventListener('click', () => { if (!capturing) setCapturing(true); });
  $('hotkey').addEventListener('blur', () => {
    if (capturing) {
      // 確定しないまま離れたら、元に戻す
      if (!$('hotkey').value) $('hotkey').value = $('hotkey').dataset.before || '';
      setCapturing(false);
    }
  });
  $('hotkey').addEventListener('keydown', (ev) => {
    if (!capturing) return;
    if (ev.key === 'Tab' && !ev.ctrlKey && !ev.altKey && !ev.metaKey) return; // 次の欄へ移動
    ev.preventDefault();
    ev.stopPropagation();
    if (ev.key === 'Escape' && !ev.ctrlKey && !ev.altKey && !ev.shiftKey && !ev.metaKey) {
      $('hotkey').value = $('hotkey').dataset.before || '';
      setCapturing(false);
      $('hotkey').blur();
      return;
    }
    const r = C.readKey(ev);
    $('hotkey').value = r.text;
    if (r.complete) {
      setCapturing(false);
      $('hotkey').blur();
    } else if (r.unsupported) {
      show('msgError', 'このキーはホットキーに使えません');
    }
  });
  $('hotkey').addEventListener('keyup', (ev) => {
    // 修飾キーだけ押して離した場合は、表示をクリアして待ち直す
    if (capturing && !C.readKey(ev).complete) $('hotkey').value = C.readKey(ev).text;
  });
  $('btnClearKey').addEventListener('click', () => { $('hotkey').value = ''; setCapturing(false); });

  // ---- 参照 ----
  async function pick(inputId, folder) {
    try {
      const p = await invoke('pick_path', { folder, current: $(inputId).value });
      if (p) $(inputId).value = p;
    } catch (e) { show('msgError', String(e)); }
  }
  $('btnPickTarget').addEventListener('click', () => pick('target', $('kind').value === 'open_folder'));
  $('btnPickCwd').addEventListener('click', () => pick('cwd', true));
  $('kind').addEventListener('change', applyKind);

  // ---- 保存・テスト・閉じる ----
  async function save() {
    clearMessages();
    $('btnSave').disabled = true;
    try {
      const r = await invoke('save_entry', { input: read() });
      entry = { ...entry, id: r.id };
      if (r.warning) {
        // 保存はできたが、ホットキーが取れなかった。画面は開いたままにして理由を見せる。
        show('msgWarn', '保存しました。ただし、このキーは登録できませんでした。\n' + r.warning + '\n別のキーに変えるか、そのまま閉じてください。');
      }
    } catch (e) {
      show('msgError', String(e));
    } finally {
      $('btnSave').disabled = false;
    }
  }

  async function test() {
    clearMessages();
    try {
      await invoke('test_entry', { input: { ...read(), hotkey: read().hotkey || 'F1' } });
      show('msgOk', '実行しました');
    } catch (e) {
      show('msgError', String(e));
    }
  }

  $('btnSave').addEventListener('click', save);
  $('btnTest').addEventListener('click', test);
  $('btnCancel').addEventListener('click', () => invoke('close_self'));
  document.addEventListener('keydown', (ev) => {
    if (capturing) return;
    if (ev.key === 'Escape') invoke('close_self');
    else if (ev.key === 'Enter' && ev.target.tagName !== 'BUTTON' && ev.target.tagName !== 'SELECT') save();
  });

  (async function init() {
    try {
      const r = await invoke('edit_target');
      entry = r.entry;
      fill(entry);
      document.title = r.is_new ? 'HotRun - 追加' : 'HotRun - 編集';
      if (r.failure) show('msgWarn', 'このキーは現在登録できていません:\n' + r.failure);
    } catch (e) {
      show('msgError', String(e));
      $('btnSave').disabled = $('btnTest').disabled = true;
    }
  })();
})();
