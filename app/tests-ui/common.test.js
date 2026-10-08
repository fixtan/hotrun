// 実行: node --test app/tests-ui/common.test.js
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const C = require('../ui/common.js');

const key = (o) => C.readKey({ ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, keyCode: 0, code: '', ...o });

test('名前の表が Rust 側（core/src/hotkey.rs）と一致する', () => {
  const src = fs.readFileSync(path.join(__dirname, '../../core/src/hotkey.rs'), 'utf8');
  const block = src.slice(src.indexOf('const NAMED_KEYS'), src.indexOf('];', src.indexOf('const NAMED_KEYS')));
  const pairs = [...block.matchAll(/\("((?:\\\\|[^"\\])+|\\\\)",\s*(\d+)\)/g)].map((m) => [m[1].replace(/\\\\/g, '\\'), Number(m[2])]);
  assert.ok(pairs.length > 40, `表を読み取れた: ${pairs.length}`);
  for (const [name, vk] of pairs) assert.strictEqual(C.nameForVk(vk), name, `VK ${vk}`);
});

test('英数字とファンクションキー', () => {
  assert.strictEqual(C.nameForVk(65), 'A');
  assert.strictEqual(C.nameForVk(57), '9');
  assert.strictEqual(C.nameForVk(112), 'F1');
  assert.strictEqual(C.nameForVk(130), 'F19');
  assert.strictEqual(C.nameForVk(135), 'F24');
  assert.strictEqual(C.nameForVk(136), null);
  assert.strictEqual(C.nameForVk(0), null);
});

test('修飾キー付きの組み合わせを、Rust の表示順（Ctrl+Alt+Shift+Win）で読む', () => {
  const r = key({ ctrlKey: true, shiftKey: true, keyCode: 49 });
  assert.deepStrictEqual([r.text, r.complete], ['Ctrl+Shift+1', true]);
  assert.strictEqual(key({ ctrlKey: true, altKey: true, shiftKey: true, metaKey: true, keyCode: 84 }).text, 'Ctrl+Alt+Shift+Win+T');
});

test('修飾キーだけの間は未確定', () => {
  const r = key({ ctrlKey: true, keyCode: 17 });
  assert.deepStrictEqual([r.text, r.complete, r.key], ['Ctrl', false, null]);
  assert.strictEqual(key({ ctrlKey: true, shiftKey: true, keyCode: 16 }).complete, false);
});

test('記号キーは keyCode（VK）で読むので、キーボード配列に左右されない', () => {
  assert.strictEqual(key({ ctrlKey: true, shiftKey: true, keyCode: 219 }).text, 'Ctrl+Shift+[');
  assert.strictEqual(key({ ctrlKey: true, shiftKey: true, keyCode: 192 }).text, 'Ctrl+Shift+`');
});

test('テンキーと単独のファンクションキー', () => {
  assert.strictEqual(key({ keyCode: 130 }).text, 'F19');
  assert.strictEqual(key({ ctrlKey: true, keyCode: 96 }).text, 'Ctrl+Num0');
});

test('IME が有効で keyCode が 229 のときは、物理キーから英数字だけ拾う', () => {
  assert.strictEqual(key({ ctrlKey: true, keyCode: 229, code: 'KeyM' }).text, 'Ctrl+M');
  assert.strictEqual(key({ ctrlKey: true, keyCode: 229, code: 'Digit3' }).text, 'Ctrl+3');
  const r = key({ ctrlKey: true, keyCode: 229, code: 'Semicolon' });
  assert.strictEqual(r.complete, false);
  assert.ok(r.unsupported);
});

test('使えないキーは unsupported', () => {
  const r = key({ keyCode: 20 }); // CapsLock
  assert.ok(r.unsupported && !r.complete);
});

test('一覧の状態表示', () => {
  const e = { id: 'e01', enabled: true };
  assert.deepStrictEqual(C.stateOf(e, {}, new Set()), { cls: 'ok', text: '使用可能', title: '' });
  const why = '他のアプリがこのキーを使用中です（どのアプリかは Windows から分かりません）';
  const ng = C.stateOf(e, { e01: why }, new Set());
  assert.deepStrictEqual([ng.cls, ng.text, ng.title], ['ng', '他のアプリが使用中', why]);
  assert.strictEqual(C.stateOf({ ...e, enabled: false }, { e01: why }, new Set()).cls, 'off', '無効なら失敗扱いにしない');
  assert.strictEqual(C.stateOf(e, {}, new Set(['e01'])).cls, 'warn');
});

test('実行内容の表示は、引数があれば続けて出す', () => {
  assert.strictEqual(C.commandText({ target: 'a.exe', args: '' }), 'a.exe');
  assert.strictEqual(C.commandText({ target: 'a.exe', args: '-x 1' }), 'a.exe -x 1');
});
