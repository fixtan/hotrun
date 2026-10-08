// 編集ウィンドウ（app/ui/edit.html）の動作テスト。
const { JSDOM } = require('jsdom');
const assert = require('node:assert');
const path = require('node:path');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const base = (o = {}) => ({ id: 'e02', hotkey: 'Ctrl+2', kind: 'open_file', target: 'D:\\a\\PuTTY2.exe', args: '', cwd: '', window: 'normal', label: 'PuTTY02.exe', group: 'g1', enabled: true, ...o });

async function boot({ entry = base(), is_new = false, failure = null, handlers = {} } = {}) {
  const calls = [];
  const invoke = async (cmd, args) => {
    calls.push([cmd, JSON.parse(JSON.stringify(args || {}))]);
    if (handlers[cmd]) return handlers[cmd](args);
    switch (cmd) {
      case 'edit_target': return { entry, is_new, failure };
      case 'save_entry': return { id: args.input.id || 'e24', warning: null };
      default: return null;
    }
  };
  const dom = await JSDOM.fromFile(path.join(__dirname, '../../ui/edit.html'), {
    runScripts: 'dangerously', resources: 'usable', pretendToBeVisual: true,
    beforeParse(w) { w.__TAURI__ = { core: { invoke } }; },
  });
  await new Promise((r) => dom.window.addEventListener('load', r));
  await sleep(50);
  const w = dom.window, d = w.document;
  const $ = (id) => d.getElementById(id);
  const press = (o) => $('hotkey').dispatchEvent(new w.KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...o }));
  const focusKey = () => $('hotkey').dispatchEvent(new w.Event('focus'));
  const type = (id, v) => { $(id).value = v; $(id).dispatchEvent(new w.Event('input', { bubbles: true })); };
  return { w, d, $, calls, press, focusKey, type };
}
const last = (calls, name) => calls.filter((c) => c[0] === name).pop();

(async () => {
  let n = 0; const ok = (m) => console.log(`ok ${++n} - ${m}`);

  // 1. 既存項目を読み込む
  let s = await boot();
  assert.strictEqual(s.$('hotkey').value, 'Ctrl+2');
  assert.strictEqual(s.$('kind').value, 'open_file');
  assert.strictEqual(s.$('target').value, 'D:\\a\\PuTTY2.exe');
  assert.strictEqual(s.$('label').value, 'PuTTY02.exe');
  assert.strictEqual(s.$('enabled').checked, true);
  assert.strictEqual(s.d.title, 'HotRun - 編集');
  ok('既存の項目が入力欄に入る');

  // 2. 保存すると、画面に出していない値（id, group）も保つ
  s.type('args', '--profile work');
  s.type('cwd', 'D:\\work');
  s.$('window').value = 'minimized';
  s.$('btnSave').click();
  await sleep(20);
  const saved = last(s.calls, 'save_entry')[1].input;
  assert.deepStrictEqual(saved, base({ args: '--profile work', cwd: 'D:\\work', window: 'minimized' }));
  ok('保存: 変更した値を送り、id と group は保つ');

  // 3. キー取り込み: 押した組み合わせがそのまま入る
  s = await boot();
  s.focusKey();
  assert.strictEqual(s.$('hotkey').value, '', 'クリックで取り込み待ちになり、欄が空になる');
  s.press({ key: 'Control', keyCode: 17, ctrlKey: true });
  assert.strictEqual(s.$('hotkey').value, 'Ctrl');
  s.press({ key: 'M', keyCode: 77, ctrlKey: true, shiftKey: true });
  assert.strictEqual(s.$('hotkey').value, 'Ctrl+Shift+M');
  assert.ok(!s.$('hotkey').classList.contains('capturing'), '押したら取り込み完了');
  s.press({ key: 'X', keyCode: 88, ctrlKey: true });
  assert.strictEqual(s.$('hotkey').value, 'Ctrl+Shift+M', '完了後は押しても変わらない');
  ok('ホットキーの取り込み');

  // 4. Esc で取り消し（ウィンドウは閉じない）
  s = await boot();
  s.focusKey();
  s.press({ key: 'Escape', keyCode: 27 });
  assert.strictEqual(s.$('hotkey').value, 'Ctrl+2');
  assert.ok(!s.calls.some((c) => c[0] === 'close_self'));
  ok('取り込み中の Esc は元に戻すだけ');

  // 5. 取り込み中でなければ Esc で閉じる
  s.d.dispatchEvent(new s.w.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await sleep(10);
  assert.ok(s.calls.some((c) => c[0] === 'close_self'));
  ok('Esc で閉じる');

  // 6. 取り込まずに離れたら元に戻る
  s = await boot();
  s.focusKey();
  s.$('hotkey').dispatchEvent(new s.w.Event('blur'));
  assert.strictEqual(s.$('hotkey').value, 'Ctrl+2');
  ok('取り込まずに離れると元のキーに戻る');

  // 7. ホットキーに使えないキー（CapsLock など）は理由を出す
  s = await boot();
  s.focusKey();
  s.press({ key: 'CapsLock', keyCode: 20 });
  assert.strictEqual(s.$('msgError').hidden, false);
  ok('使えないキーは理由を出す');

  // 8. 動作による出し分け
  s = await boot({ entry: base({ kind: 'run' }) });
  assert.strictEqual(s.$('argsField').hidden, false);
  assert.strictEqual(s.$('targetLabel').textContent, 'プログラム');
  s.$('kind').value = 'open_folder'; s.$('kind').dispatchEvent(new s.w.Event('change'));
  assert.strictEqual(s.$('argsField').hidden, true);
  assert.strictEqual(s.$('windowField').hidden, true);
  assert.strictEqual(s.$('targetLabel').textContent, 'フォルダ');
  ok('フォルダを開くときは引数などを隠す');

  // 9. 参照: 動作に合わせてファイル/フォルダを選ぶ
  s = await boot({ handlers: { pick_path: async (a) => (a.folder ? 'D:\\picked' : 'D:\\picked.exe') } });
  s.$('btnPickTarget').click(); await sleep(10);
  assert.strictEqual(s.$('target').value, 'D:\\picked.exe');
  assert.deepStrictEqual(last(s.calls, 'pick_path')[1], { folder: false, current: 'D:\\a\\PuTTY2.exe' });
  s.$('btnPickCwd').click(); await sleep(10);
  assert.strictEqual(s.$('cwd').value, 'D:\\picked');
  s.$('kind').value = 'open_folder'; s.$('kind').dispatchEvent(new s.w.Event('change'));
  s.$('btnPickTarget').click(); await sleep(10);
  assert.strictEqual(last(s.calls, 'pick_path')[1].folder, true);
  ok('参照ボタン');

  // 10. 参照をキャンセルしたら入力は変えない
  s = await boot({ handlers: { pick_path: async () => null } });
  s.$('btnPickTarget').click(); await sleep(10);
  assert.strictEqual(s.$('target').value, 'D:\\a\\PuTTY2.exe');
  ok('参照キャンセルでは変えない');

  // 11. 保存エラーは画面に出して閉じない
  s = await boot({ handlers: { save_entry: async () => { throw 'Ctrl+2 はすでに「PuTTY.exe」に割り当てられています'; } } });
  s.$('btnSave').click(); await sleep(20);
  assert.strictEqual(s.$('msgError').hidden, false);
  assert.match(s.$('msgError').textContent, /すでに「PuTTY\.exe」/);
  assert.strictEqual(s.$('btnSave').disabled, false, '直して再保存できる');
  ok('保存エラーを表示');

  // 12. 保存はできたがキーが取れない → 警告。新規は id を覚えて二重登録しない
  s = await boot({ entry: base({ id: '', hotkey: 'Ctrl+9' }), is_new: true,
    handlers: { save_entry: async (a) => ({ id: 'e24', warning: '他のアプリがこのキーを使用中です' }) } });
  s.$('btnSave').click(); await sleep(20);
  assert.strictEqual(s.$('msgWarn').hidden, false);
  assert.match(s.$('msgWarn').textContent, /保存しました.*他のアプリ/s);
  s.$('btnSave').click(); await sleep(20);
  const saves = s.calls.filter((c) => c[0] === 'save_entry');
  assert.deepStrictEqual([saves[0][1].input.id, saves[1][1].input.id], ['', 'e24']);
  ok('警告つきで保存した新規項目は、再保存で更新になる');

  // 13. 新規の初期値
  s = await boot({ entry: { id: '', hotkey: '', kind: 'run', target: '', args: '', cwd: '', window: 'normal', label: '', group: '', enabled: true }, is_new: true });
  assert.strictEqual(s.d.title, 'HotRun - 追加');
  assert.strictEqual(s.$('hotkey').value, '');
  assert.strictEqual(s.$('kind').value, 'run');
  assert.strictEqual(s.$('enabled').checked, true);
  ok('新規の初期値');

  // 14. テスト実行は保存せず、入力中の内容を送る
  s = await boot();
  s.type('args', '/x');
  s.$('btnTest').click(); await sleep(20);
  assert.ok(!s.calls.some((c) => c[0] === 'save_entry'));
  assert.strictEqual(last(s.calls, 'test_entry')[1].input.args, '/x');
  assert.strictEqual(s.$('msgOk').hidden, false);
  ok('テスト実行');

  // 15. テスト実行の失敗
  s = await boot({ handlers: { test_entry: async () => { throw 'ファイルが見つかりません'; } } });
  s.$('btnTest').click(); await sleep(20);
  assert.match(s.$('msgError').textContent, /ファイルが見つかりません/);
  ok('テスト実行の失敗を表示');

  // 16. 現在キーが取れていない項目は、開いた時点で理由を見せる
  s = await boot({ failure: '他のアプリがこのキーを使用中です' });
  assert.match(s.$('msgWarn').textContent, /現在登録できていません.*他のアプリ/s);
  ok('開いた時点で登録失敗の理由を出す');

  // 17. 項目が消えていたら保存できない
  s = await boot({ handlers: { edit_target: async () => { throw 'この項目はすでに削除されています'; } } });
  assert.strictEqual(s.$('btnSave').disabled, true);
  assert.match(s.$('msgError').textContent, /削除/);
  ok('消えた項目は開けない');

  // 18. クリアと Enter 保存
  s = await boot();
  s.$('btnClearKey').click();
  assert.strictEqual(s.$('hotkey').value, '');
  s.$('target').dispatchEvent(new s.w.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await sleep(20);
  assert.ok(s.calls.some((c) => c[0] === 'save_entry'));
  ok('クリアと Enter で保存');

  console.log(`\n${n} passed`);
  process.exit(0);
})().catch((e) => { console.error(e); process.exit(1); });
