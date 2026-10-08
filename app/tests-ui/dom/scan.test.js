// 使用中のキー一覧（app/ui/scan.html）の動作テスト。
const { JSDOM } = require('jsdom');
const assert = require('node:assert');
const path = require('node:path');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const T = (hotkey, cls) => ({ hotkey, mods: 0, vk: 0, class: cls });
const sample = () => [T('Ctrl+1', 'hotrun'), T('Win+E', 'win'), T('Win+Shift+S', 'win'), T('Ctrl+Shift+M', 'other'), T('Alt+F1', 'other')];

async function boot({ items = sample(), failed = [], scanError = null } = {}) {
  const calls = [];
  let n = 0;
  const invoke = async (cmd, args) => {
    calls.push([cmd, args]);
    if (cmd === 'get_state') return { failed };
    if (cmd === 'scan_hotkeys') {
      n++;
      if (scanError) throw scanError;
      return items.slice(0, n === 1 ? items.length : items.length - 1);
    }
    return null;
  };
  const dom = await JSDOM.fromFile(path.join(__dirname, '../../ui/scan.html'), {
    runScripts: 'dangerously', resources: 'usable', pretendToBeVisual: true,
    beforeParse(w) { w.__TAURI__ = { core: { invoke } }; },
  });
  await new Promise((r) => dom.window.addEventListener('load', r));
  await sleep(50);
  const w = dom.window, d = w.document;
  const group = (cls) => d.querySelector(`details[data-cls="${cls}"]`);
  const chips = (cls) => [...group(cls).querySelectorAll('kbd')].map((k) => k.dataset.hotkey);
  return { w, d, calls, group, chips };
}

(async () => {
  let k = 0; const ok = (m) => console.log(`ok ${++k} - ${m}`);

  // 1. 3 つに分けて出す
  let s = await boot();
  assert.deepStrictEqual(s.chips('hotrun'), ['Ctrl+1']);
  assert.deepStrictEqual(s.chips('win'), ['Win+E', 'Win+Shift+S']);
  assert.deepStrictEqual(s.chips('other'), ['Ctrl+Shift+M', 'Alt+F1']);
  assert.match(s.d.getElementById('summary').textContent, /合計 5 件/);
  assert.match(s.group('other').querySelector('summary').textContent, /他のアプリが使用中（2 件）/);
  ok('HotRun / Win キー系 / 他のアプリ に分けて出す');

  // 2. Win 系は折りたたみ、他は開く
  assert.strictEqual(s.group('win').open, false);
  assert.strictEqual(s.group('other').open, true);
  ok('Win 系は折りたたんで始める');

  // 3. 設定で取れなかったキーは赤く出し、上にも書く
  s = await boot({ failed: [{ id: 'e19', key: 'Ctrl+Shift+M', why: 'x' }] });
  const mine = [...s.d.querySelectorAll('kbd.mine')].map((e) => e.dataset.hotkey);
  assert.deepStrictEqual(mine, ['Ctrl+Shift+M']);
  assert.strictEqual(s.d.getElementById('mine').hidden, false);
  assert.match(s.d.getElementById('mine').textContent, /Ctrl\+Shift\+M/);
  ok('取れなかったキーを目立たせる');

  // 4. 取れなかったキーが一覧に無ければ、バナーは出さない
  s = await boot({ failed: [{ id: 'e01', key: 'Ctrl+9', why: 'x' }] });
  assert.strictEqual(s.d.getElementById('mine').hidden, true);
  ok('一覧に無いときはバナーなし');

  // 5. 絞り込み
  s = await boot();
  const f = s.d.getElementById('filter');
  f.value = 'shift'; f.dispatchEvent(new s.w.Event('input'));
  assert.deepStrictEqual(s.chips('other'), ['Ctrl+Shift+M']);
  assert.deepStrictEqual(s.chips('win'), ['Win+Shift+S']);
  assert.strictEqual(s.group('win').open, true, '絞り込み中は Win 系も開く');
  assert.match(s.group('other').querySelector('summary').textContent, /1 \/ 2 件/);
  f.value = 'zzz'; f.dispatchEvent(new s.w.Event('input'));
  assert.deepStrictEqual(s.chips('other'), []);
  ok('絞り込み');

  // 6. 再スキャン
  s = await boot();
  s.d.getElementById('btnRescan').click();
  await sleep(30);
  assert.strictEqual(s.calls.filter((c) => c[0] === 'scan_hotkeys').length, 2);
  assert.match(s.d.getElementById('summary').textContent, /合計 4 件/);
  assert.strictEqual(s.d.getElementById('btnRescan').disabled, false);
  ok('再スキャンで結果が入れ替わる');

  // 7. 失敗したらエラーを出す
  s = await boot({ scanError: 'Windows でのみ使えます' });
  assert.strictEqual(s.d.getElementById('error').hidden, false);
  assert.match(s.d.getElementById('error').textContent, /Windows でのみ/);
  assert.strictEqual(s.d.querySelectorAll('details').length, 0);
  ok('スキャンの失敗を表示');

  // 8. Esc で閉じる
  s = await boot();
  s.d.dispatchEvent(new s.w.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await sleep(10);
  assert.ok(s.calls.some((c) => c[0] === 'close_self'));
  ok('Esc で閉じる');

  // 9. 押して確認: 届いたキー / 届かなかったキー
  s = await boot();
  const probe = s.d.getElementById('probe');
  const press = (init) => s.d.dispatchEvent(new s.w.KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }));
  press({ keyCode: 70, ctrlKey: true, shiftKey: true }); // 押して確認の前は何も起きない
  assert.strictEqual(probe.hidden, true);
  s.d.getElementById('btnProbe').click();
  assert.strictEqual(probe.hidden, false);
  assert.match(probe.textContent, /今押してください/);
  press({ keyCode: 17, ctrlKey: true }); // Ctrl だけ。まだ待つ
  assert.match(probe.textContent, /今押してください/);
  press({ keyCode: 70, ctrlKey: true, shiftKey: true });
  assert.match(probe.textContent, /Ctrl\+Shift\+F はこの画面に届きました。今は誰も取っていません/);
  assert.strictEqual(s.d.getElementById('btnProbe').textContent, '押して確認');
  ok('押して確認: 届いたキーは「誰も取っていない」');

  s.d.getElementById('btnProbe').click();
  press({ keyCode: 77, ctrlKey: true, shiftKey: true }); // 一覧では他のアプリが使用中のはずのキー
  assert.match(probe.textContent, /他のアプリが使用中」に入っています/);
  s.d.getElementById('btnProbe').click();
  press({ key: 'Escape', keyCode: 27 }); // 確認中の Esc は閉じずに、キーとして調べる
  assert.ok(!s.calls.some((c) => c[0] === 'close_self'));
  assert.match(probe.textContent, /Esc はこの画面に届きました/);
  s.d.getElementById('btnProbe').click();
  s.d.getElementById('btnProbe').click(); // 中止
  assert.strictEqual(probe.hidden, true);
  ok('押して確認: 一覧と食い違えば知らせる・中止できる');

  s = await boot();
  s.w.setTimeout = ((orig) => (fn, ms) => orig(fn, ms === 5000 ? 30 : ms))(s.w.setTimeout.bind(s.w));
  s.d.getElementById('btnProbe').click();
  await sleep(80);
  assert.match(s.d.getElementById('probe').textContent, /届きませんでした/);
  assert.match(s.d.getElementById('probe').className, /warn/);
  ok('押して確認: 届かなければ「先約あり」');

  console.log(`\n${k} passed`);
  process.exit(0);
})().catch((e) => { console.error(e); process.exit(1); });
