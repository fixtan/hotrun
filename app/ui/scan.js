// 使用中のキー一覧ウィンドウ
(function () {
  const { invoke } = window.__TAURI__.core;
  const $ = (id) => document.getElementById(id);

  const GROUPS = [
    { cls: 'hotrun', title: 'HotRun が使用中', note: 'この設定のキーです。' },
    { cls: 'win', title: 'Win キーを含む組み合わせ', note: 'Windows 標準、または PowerToys など Win キーを使うアプリです。' },
    { cls: 'other', title: '他のアプリが使用中', note: 'どのアプリかは Windows からは分かりません。実際に押して確かめてください。' },
  ];

  let items = [];
  let mineFailed = new Set(); // 設定には書いてあるのに、取れなかったキー

  function h(tag, cls, t) {
    const el = document.createElement(tag);
    if (cls) el.className = cls;
    if (t !== undefined) el.textContent = t;
    return el;
  }

  // 修飾キーの部分（Hotkey の表示順: Ctrl, Alt, Shift, Win）。empty を渡すと、キー名の前に付く接頭辞（"Ctrl+Alt+"）を返す
  function modsLabel(m, none) {
    const p = [];
    if (m & 2) p.push('Ctrl');
    if (m & 1) p.push('Alt');
    if (m & 4) p.push('Shift');
    if (m & 8) p.push('Win');
    if (none !== undefined) return p.map((x) => x + '+').join('');
    return p.length ? p.join(' + ') : '修飾キーなし';
  }

  function render() {
    const q = $('filter').value.trim().toLowerCase();
    const shown = items.filter((t) => !q || t.hotkey.toLowerCase().includes(q));
    const root = $('groups');
    root.textContent = '';
    for (const g of GROUPS) {
      const all = items.filter((t) => t.class === g.cls);
      const list = shown.filter((t) => t.class === g.cls);
      if (all.length === 0) continue;
      const sec = h('details', 'group');
      sec.open = g.cls !== 'win' || !!q;
      sec.dataset.cls = g.cls;
      const sum = h('summary', '', `${g.title}（${q ? `${list.length} / ` : ''}${all.length} 件）`);
      sec.appendChild(sum);
      sec.appendChild(h('p', 'dim note', g.note));
      const box = h('div', 'rows');
      let row = null;
      let last = -1;
      for (const t of list) {
        if (t.mods !== last) {
          last = t.mods;
          row = h('div', 'row');
          row.appendChild(h('div', 'mods', modsLabel(t.mods)));
          row.appendChild(h('div', 'chips'));
          box.appendChild(row);
        }
        const key = t.hotkey.slice(modsLabel(t.mods, '').length);
        const chip = h('kbd', mineFailed.has(t.hotkey) ? 'mine' : '', key);
        chip.dataset.hotkey = t.hotkey;
        chip.title = mineFailed.has(t.hotkey) ? `${t.hotkey}（あなたの設定で取れなかったキー）` : t.hotkey;
        row.lastChild.appendChild(chip);
      }
      sec.appendChild(box);
      root.appendChild(sec);
    }
    $('summary').textContent = items.length ? `合計 ${items.length} 件` : '';
  }

  async function scan() {
    $('btnRescan').disabled = true;
    $('error').hidden = true;
    $('summary').textContent = '調べています…';
    try {
      const st = await invoke('get_state');
      mineFailed = new Set(st.failed.map((f) => f.key));
      items = await invoke('scan_hotkeys');
      const hit = items.filter((t) => mineFailed.has(t.hotkey)).map((t) => t.hotkey);
      $('mine').hidden = hit.length === 0;
      $('mine').textContent = hit.length ? `あなたの設定で取れなかったキー: ${hit.join(', ')}（下の一覧で赤く表示）` : '';
      render();
    } catch (e) {
      items = [];
      render();
      $('error').hidden = false;
      $('error').textContent = String(e);
      $('summary').textContent = '';
    } finally {
      $('btnRescan').disabled = false;
    }
  }

  // 押して確認: 他のアプリが取っているキーは、押してもこの画面には届かない（持ち主のアプリが反応する）。
  const PROBE_MS = 5000;
  let probing = null; // { timer }

  function probeEnd(msg, cls) {
    if (probing) clearTimeout(probing.timer);
    probing = null;
    $('btnProbe').textContent = '押して確認';
    const el = $('probe');
    el.hidden = !msg;
    el.textContent = msg || '';
    el.className = 'banner' + (cls ? ' ' + cls : '');
  }

  function probeStart() {
    if (probing) return probeEnd('');
    $('btnProbe').textContent = '中止';
    const el = $('probe');
    el.hidden = false;
    el.className = 'banner';
    el.textContent = `調べたいキーを今押してください（${PROBE_MS / 1000} 秒以内）。`;
    probing = {
      timer: setTimeout(() => probeEnd(
        '押したキーはこの画面に届きませんでした。他のアプリが取っているか、Windows が予約しているキーです。押した瞬間に反応したアプリが持ち主です（そのアプリの動作が実行されます）。', 'warn'), PROBE_MS),
    };
  }

  document.addEventListener('keydown', (ev) => {
    if (!probing) return;
    const k = window.HotRunCommon.readKey(ev);
    if (!k.complete) return; // Ctrl などの修飾キーだけ。続きを待つ
    ev.preventDefault();
    ev.stopImmediatePropagation(); // 確認中の Esc は閉じずに、キーとして調べる
    const t = items.find((x) => x.hotkey === k.text);
    if (t) {
      const g = GROUPS.find((x) => x.cls === t.class).title;
      probeEnd(`${k.text} はこの画面に届きましたが、一覧では「${g}」に入っています。再スキャンしてください。`, 'warn');
    } else {
      probeEnd(`${k.text} はこの画面に届きました。今は誰も取っていません。`);
    }
  }, true);

  $('btnProbe').addEventListener('click', probeStart);
  $('btnRescan').addEventListener('click', scan);
  $('filter').addEventListener('input', render);
  document.addEventListener('keydown', (ev) => {
    if (ev.key === 'Escape' && !probing) invoke('close_self');
  });
  scan();
})();
