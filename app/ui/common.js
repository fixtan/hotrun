// 画面共通の関数（DOM に触らないので Node でもテストできる）
(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.HotRunCommon = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  // 仮想キーコード → 名前。名前は core/src/hotkey.rs の NAMED_KEYS と同じ。
  const NAMED = {
    8: 'Backspace', 9: 'Tab', 13: 'Enter', 19: 'Pause', 27: 'Esc', 32: 'Space', 33: 'PageUp', 34: 'PageDown',
    35: 'End', 36: 'Home', 37: 'Left', 38: 'Up', 39: 'Right', 40: 'Down', 44: 'PrintScreen', 45: 'Insert', 46: 'Delete',
    96: 'Num0', 97: 'Num1', 98: 'Num2', 99: 'Num3', 100: 'Num4', 101: 'Num5', 102: 'Num6', 103: 'Num7', 104: 'Num8', 105: 'Num9',
    106: 'Num*', 107: 'Num+', 109: 'Num-', 110: 'Num.', 111: 'Num/',
    166: 'BrowserBack', 167: 'BrowserForward', 168: 'BrowserRefresh', 169: 'BrowserStop', 170: 'BrowserSearch', 171: 'BrowserFavorites', 172: 'BrowserHome', 173: 'VolumeMute', 174: 'VolumeDown', 175: 'VolumeUp', 176: 'MediaNext', 177: 'MediaPrev', 178: 'MediaStop', 179: 'MediaPlayPause', 180: 'LaunchMail', 181: 'LaunchMedia', 182: 'LaunchApp1', 183: 'LaunchApp2',
    186: ';', 187: '=', 188: ',', 189: '-', 190: '.', 191: '/', 192: '`', 219: '[', 220: '\\', 221: ']', 222: "'",
  };
  const MODIFIER_CODES = new Set([16, 17, 18, 91, 92, 93]); // Shift, Ctrl, Alt, Win(左右), Menu

  function nameForVk(vk) {
    if ((vk >= 48 && vk <= 57) || (vk >= 65 && vk <= 90)) return String.fromCharCode(vk);
    if (vk >= 112 && vk <= 135) return 'F' + (vk - 111);
    return NAMED[vk] || null;
  }

  // IME が有効だと keyCode が 229 になるので、物理キー(code)から文字・数字だけ拾い直す
  function vkFromCode(code) {
    let m = /^Key([A-Z])$/.exec(code || '');
    if (m) return m[1].charCodeAt(0);
    m = /^Digit(\d)$/.exec(code || '');
    if (m) return m[1].charCodeAt(0);
    return 0;
  }

  /** keydown イベントからホットキーを読む。{ mods, key, text, complete } */
  function readKey(e) {
    const mods = [];
    if (e.ctrlKey) mods.push('Ctrl');
    if (e.altKey) mods.push('Alt');
    if (e.shiftKey) mods.push('Shift');
    if (e.metaKey) mods.push('Win');
    let vk = e.keyCode || 0;
    if (vk === 229) vk = vkFromCode(e.code);
    if (MODIFIER_CODES.has(vk)) return { mods, key: null, text: mods.join('+'), complete: false };
    const key = nameForVk(vk);
    if (!key) return { mods, key: null, text: mods.join('+'), complete: false, unsupported: true };
    return { mods, key, text: [...mods, key].join('+'), complete: true };
  }

  const KIND_LABEL = { run: 'プログラムを実行', open_file: 'ファイルを開く', open_folder: 'フォルダを開く' };
  const WINDOW_LABEL = { normal: '通常', maximized: '最大化', minimized: '最小化', hidden: '非表示' };

  /** 一覧の「実行内容」欄 */
  function commandText(e) {
    return e.args ? `${e.target} ${e.args}` : e.target;
  }

  /** 一覧の「状態」欄。{ cls, text, title } */
  function stateOf(e, failedById, issueIds) {
    if (!e.enabled) return { cls: 'off', text: '無効', title: '' };
    const f = failedById[e.id];
    if (f) {
      const short = f.startsWith('他のアプリ') ? '他のアプリが使用中' : f;
      return { cls: 'ng', text: short, title: f };
    }
    if (issueIds.has(e.id)) return { cls: 'warn', text: '要確認（上の注意を参照）', title: '' };
    return { cls: 'ok', text: '使用可能', title: '' };
  }

  return { nameForVk, readKey, commandText, stateOf, KIND_LABEL, WINDOW_LABEL };
});
