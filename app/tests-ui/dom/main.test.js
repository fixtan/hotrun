// 一覧ウィンドウ（app/ui/index.html）の動作テスト。jsdom が必要: `cd app/tests-ui && npm install && npm run test:dom`
const { JSDOM } = require('jsdom');
const assert = require('node:assert');
const path = require('node:path');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const NG = '他のアプリがこのキーを使用中です（どのアプリかは Windows から分かりません）';
const E = (id, hotkey, target, o = {}) => ({ id, hotkey, kind: 'open_file', target, args: '', cwd: '', window: 'normal', label: '', group: '', enabled: true, name: o.label || target.split('\\').pop(), ...o });

async function boot({ entries, failed = [], issues = [], load_error = null, failOn = {} } = {}) {
  const calls = [];
  const st = {
    path: 'C:\\Users\\x\\AppData\\Roaming\\hotrun\\config.json', load_error,
    entries: entries || [E('e01', 'Ctrl+1', 'D:\\a\\PuTTY.exe'), E('e02', 'Ctrl+Shift+M', 'D:\\a\\窓.exe', { args: '--x' }), E('e03', 'F19', 'D:\\b\\off.exe', { enabled: false })],
    registered: 2, failed, issues, autostart: false,
  };
  const listeners = {};
  const invoke = async (cmd, args) => {
    calls.push([cmd, JSON.parse(JSON.stringify(args || {}))]);
    if (failOn[cmd]) throw failOn[cmd];
    switch (cmd) {
      case 'get_state': return JSON.parse(JSON.stringify(st));
      case 'delete_entry': st.entries = st.entries.filter((e) => e.id !== args.id); return null;
      case 'set_enabled': st.entries.find((e) => e.id === args.id).enabled = args.enabled; return null;
      case 'set_autostart': st.autostart = args.enable; return null;
      default: return null;
    }
  };
  const dom = await JSDOM.fromFile(path.join(__dirname, '../../ui/index.html'), {
    runScripts: 'dangerously', resources: 'usable', pretendToBeVisual: true,
    beforeParse(w) { w.__TAURI__ = { core: { invoke }, event: { listen: (n, f) => { listeners[n] = f; } } }; },
  });
  await new Promise((r) => dom.window.addEventListener('load', r));
  await sleep(50);
  const w = dom.window, d = w.document;
  const rows = () => [...d.querySelectorAll('#rows tr')];
  const click = (el) => el.dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
  const dbl = (el) => el.dispatchEvent(new w.MouseEvent('dblclick', { bubbles: true }));
  const key = (k) => d.dispatchEvent(new w.KeyboardEvent('keydown', { key: k, bubbles: true }));
  return { w, d, st, calls, rows, click, dbl, key, listeners };
}
const names = (calls) => calls.map((c) => c[0]);

(async () => {
  let n = 0; const ok = (m) => console.log(`ok ${++n} - ${m}`);

  // 1. 初期表示
  let s = await boot();
  assert.strictEqual(s.rows().length, 3);
  assert.strictEqual(s.rows()[0].querySelector('kbd').textContent, 'Ctrl+1');
  assert.strictEqual(s.rows()[1].querySelector('.cmd').textContent, 'D:\\a\\窓.exe --x', '引数も一覧に出る');
  assert.strictEqual(s.rows()[0].querySelector('.state').textContent, '使用可能');
  assert.strictEqual(s.rows()[2].querySelector('.state').textContent, '無効');
  assert.ok(s.rows()[2].classList.contains('off'));
  assert.match(s.d.getElementById('summary').textContent, /3 件（使用可能 2 件）/);
  assert.strictEqual(s.d.getElementById('empty').hidden, true);
  assert.ok(s.d.getElementById('btnEdit').disabled && s.d.getElementById('btnDelete').disabled, '未選択なら編集・削除は押せない');
  ok('一覧にホットキーと実行内容が出る');

  // 2. 取れなかったキー
  s = await boot({ failed: [{ id: 'e02', key: 'Ctrl+Shift+M', why: NG }] });
  const st2 = s.rows()[1].querySelector('.state');
  assert.strictEqual(st2.textContent, '他のアプリが使用中');
  assert.strictEqual(st2.title, NG);
  assert.ok(st2.classList.contains('ng'));
  assert.strictEqual(s.d.getElementById('bannerIssues').hidden, false);
  assert.match(s.d.getElementById('bannerIssues').textContent, /1 件のキーを登録できませんでした/);
  ok('登録できなかったキーを赤く出し、理由をツールチップに出す');

  // 3. 重複などの注意
  s = await boot({ issues: ['Ctrl+1 が重複しています: e01, e02'] });
  assert.strictEqual(s.rows()[0].querySelector('.state').className, 'state warn');
  assert.strictEqual(s.rows()[1].querySelector('.state').className, 'state warn');
  assert.strictEqual(s.rows()[2].querySelector('.state').className, 'state off', '関係ない行は変わらない');
  ok('重複の注意が、関係する行に付く');

  // 4. 選択とボタン
  s = await boot();
  s.click(s.rows()[1]);
  assert.ok(s.rows()[1].classList.contains('sel'));
  assert.ok(!s.d.getElementById('btnEdit').disabled && !s.d.getElementById('btnDelete').disabled);
  s.click(s.rows()[0]);
  assert.ok(!s.rows()[1].classList.contains('sel') && s.rows()[0].classList.contains('sel'));
  ok('行を選ぶと編集・削除が押せる');

  // 5. 編集ウィンドウを開く（ダブルクリック／編集／Enter）と追加
  s = await boot();
  s.dbl(s.rows()[1]);
  await sleep(20);
  assert.deepStrictEqual(s.calls.filter((c) => c[0] === 'open_edit'), [['open_edit', { id: 'e02' }]]);
  s.click(s.rows()[0]);
  s.d.getElementById('btnEdit').click();
  s.key('Enter');
  s.d.getElementById('btnAdd').click();
  await sleep(20);
  assert.deepStrictEqual(s.calls.filter((c) => c[0] === 'open_edit').map((c) => c[1].id), ['e02', 'e01', 'e01', '']);
  ok('ダブルクリック・編集・Enter で個別ウィンドウ、追加は新規');

  // 6. 削除は確認してから
  s = await boot();
  s.click(s.rows()[0]);
  s.d.getElementById('btnDelete').click();
  await sleep(10);
  assert.strictEqual(s.d.getElementById('overlay').hidden, false);
  assert.match(s.d.getElementById('dlgText').textContent, /PuTTY\.exe.*Ctrl\+1/);
  assert.ok(!names(s.calls).includes('delete_entry'), '確認前は消さない');
  s.d.getElementById('dlgCancel').click();
  await sleep(10);
  assert.ok(!names(s.calls).includes('delete_entry'));
  assert.strictEqual(s.d.getElementById('overlay').hidden, true);
  s.d.getElementById('btnDelete').click();
  await sleep(10);
  s.d.getElementById('dlgOk').click();
  await sleep(30);
  assert.deepStrictEqual(s.calls.filter((c) => c[0] === 'delete_entry'), [['delete_entry', { id: 'e01' }]]);
  assert.strictEqual(s.rows().length, 2);
  ok('削除は確認ダイアログを通る（キャンセルなら消えない）');

  // 7. Delete キー、矢印キー
  s = await boot();
  s.key('ArrowDown');
  assert.ok(s.rows()[0].classList.contains('sel'));
  s.key('ArrowDown'); s.key('ArrowDown'); s.key('ArrowDown');
  assert.ok(s.rows()[2].classList.contains('sel'), '端で止まる');
  s.key('ArrowUp');
  assert.ok(s.rows()[1].classList.contains('sel'));
  s.key('Delete');
  await sleep(10);
  assert.strictEqual(s.d.getElementById('overlay').hidden, false);
  s.key('Escape');
  await sleep(10);
  assert.strictEqual(s.d.getElementById('overlay').hidden, true);
  assert.ok(!names(s.calls).includes('delete_entry'));
  ok('キーボードで選択・削除（Esc で取り消し）');

  // 8. 有効/無効の切り替えは行選択を起こさない
  s = await boot();
  const cb = s.rows()[2].querySelector('input[type=checkbox]');
  cb.click();
  await sleep(10);
  assert.deepStrictEqual(s.calls.filter((c) => c[0] === 'set_enabled'), [['set_enabled', { id: 'e03', enabled: true }]]);
  assert.ok(!s.rows()[2].classList.contains('sel'));
  ok('チェックで有効/無効を切り替える');

  // 9. 切り替えに失敗したらチェックを戻して理由を見せる
  s = await boot({ failOn: { set_enabled: 'Ctrl+1 はすでに「PuTTY.exe」に割り当てられています' } });
  const cb2 = s.rows()[2].querySelector('input[type=checkbox]');
  cb2.click();
  await sleep(20);
  assert.strictEqual(cb2.checked, false);
  assert.match(s.d.getElementById('dlgText').textContent, /すでに「PuTTY\.exe」/);
  assert.strictEqual(s.d.getElementById('dlgCancel').hidden, true, '通知は OK だけ');
  ok('有効化に失敗したら戻して理由を出す');

  // 10. 設定ファイルを開く・再読み込み・自動起動
  s = await boot();
  s.d.getElementById('btnOpenFile').click();
  s.d.getElementById('btnReload').click();
  await sleep(30);
  const ch = s.d.getElementById('chkAutostart'); ch.checked = true; ch.dispatchEvent(new s.w.Event('change', { bubbles: true }));
  await sleep(30);
  assert.ok(names(s.calls).includes('open_config_file') && names(s.calls).includes('reload_config'));
  assert.deepStrictEqual(s.calls.find((c) => c[0] === 'set_autostart'), ['set_autostart', { enable: true }]);
  ok('設定ファイルを開く／再読み込み／自動起動');

  // 11. 読み込みエラーのバナー、空の一覧
  s = await boot({ entries: [], load_error: 'C:\\x\\config.json\nJSON の形式が正しくありません' });
  assert.strictEqual(s.d.getElementById('bannerError').hidden, false);
  assert.match(s.d.getElementById('bannerError').textContent, /JSON の形式/);
  assert.strictEqual(s.d.getElementById('empty').hidden, false);
  ok('読み込みエラーと空の一覧');

  // 12. 変更通知で再描画
  s = await boot();
  s.st.entries.push(E('e04', 'Ctrl+9', 'D:\\z.exe'));
  await s.listeners['config-changed']();
  await sleep(20);
  assert.strictEqual(s.rows().length, 4);
  ok('config-changed で一覧が更新される');

  // 13. 表示に使う文字列は HTML として解釈しない
  s = await boot({ entries: [E('e01', 'Ctrl+1', 'x', { name: '<img src=x onerror=window.__pwn=1>' })] });
  assert.strictEqual(s.rows()[0].querySelector('img'), null);
  assert.strictEqual(s.w.__pwn, undefined);
  ok('名前に HTML が入っていても実行されない');

  console.log(`\n${n} passed`);
  process.exit(0);
})().catch((e) => { console.error(e); process.exit(1); });
