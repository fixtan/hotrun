// 一覧ウィンドウ
(function () {
  const C = window.HotRunCommon;
  const { invoke } = window.__TAURI__.core;
  const $ = (id) => document.getElementById(id);

  let state = null;
  let selected = null; // 選択中の項目 id

  // 確認ダイアログ。cancelLabel を null にすると OK だけの通知になる。
  function dialog(message, { ok = 'OK', cancel = 'キャンセル', danger = false } = {}) {
    return new Promise((resolve) => {
      $('dlgText').textContent = message;
      $('dlgOk').textContent = ok;
      $('dlgOk').className = danger ? 'danger' : 'primary';
      $('dlgCancel').hidden = cancel === null;
      if (cancel !== null) $('dlgCancel').textContent = cancel;
      $('overlay').hidden = false;
      $('dlgOk').focus();
      const done = (v) => {
        $('overlay').hidden = true;
        $('dlgOk').onclick = $('dlgCancel').onclick = null;
        document.removeEventListener('keydown', onKey, true);
        resolve(v);
      };
      const onKey = (ev) => {
        if (ev.key === 'Escape') { ev.preventDefault(); ev.stopPropagation(); done(false); }
        else if (ev.key === 'Enter') { ev.preventDefault(); ev.stopPropagation(); done(true); }
      };
      document.addEventListener('keydown', onKey, true);
      $('dlgOk').onclick = () => done(true);
      $('dlgCancel').onclick = () => done(false);
    });
  }
  const info = (message) => dialog(message, { cancel: null });

  function text(el, t) { el.textContent = t; return el; }
  function h(tag, cls, t) {
    const el = document.createElement(tag);
    if (cls) el.className = cls;
    if (t !== undefined) el.textContent = t;
    return el;
  }

  function render() {
    const entries = state.entries;
    const failedById = Object.fromEntries(state.failed.map((f) => [f.id, f.why]));
    // 問題（重複など）に関わる項目 id を拾う。Issue は文章なので、id が含まれるかで判定する。
    const issueIds = new Set(entries.filter((e) => state.issues.some((i) => hasId(i, e.id))).map((e) => e.id));

    if (selected && !entries.some((e) => e.id === selected)) selected = null;

    const tbody = $('rows');
    tbody.textContent = '';
    for (const e of entries) {
      const tr = h('tr', (e.enabled ? '' : 'off ') + (e.id === selected ? 'sel' : ''));
      tr.dataset.id = e.id;

      const tdOn = h('td', 'c-on');
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = e.enabled;
      cb.title = '有効／無効';
      cb.addEventListener('click', (ev) => ev.stopPropagation());
      cb.addEventListener('change', () => setEnabled(e.id, cb.checked, cb));
      tdOn.appendChild(cb);

      const tdKey = h('td', 'c-key');
      tdKey.appendChild(h('kbd', '', e.hotkey));

      const tdWhat = h('td', 'c-what');
      tdWhat.appendChild(h('div', 'name', e.name));
      const cmd = h('div', 'cmd', C.commandText(e));
      cmd.title = C.commandText(e);
      tdWhat.appendChild(cmd);

      const st = C.stateOf(e, failedById, issueIds);
      const tdState = h('td', 'c-state');
      const s = h('div', 'state ' + st.cls, st.text);
      if (st.title) s.title = st.title;
      tdState.appendChild(s);

      tr.append(tdOn, tdKey, tdWhat, tdState);
      tr.addEventListener('click', () => select(e.id));
      tr.addEventListener('dblclick', () => edit(e.id));
      tbody.appendChild(tr);
    }

    $('empty').hidden = entries.length > 0;
    $('btnEdit').disabled = $('btnDelete').disabled = !selected;

    const err = $('bannerError');
    err.hidden = !state.load_error;
    if (state.load_error) text(err, '設定ファイルを読み込めません。JSON を直して「再読み込み」してください（それまで変更はできません）。\n' + state.load_error);

    const nf = state.failed.length;
    const bi = $('bannerIssues');
    const lines = [];
    if (nf > 0) lines.push(`${nf} 件のキーを登録できませんでした。他のアプリが同じキーを使っている可能性があります（行の「状態」を参照）。`);
    for (const i of state.issues) lines.push(i);
    bi.hidden = lines.length === 0;
    text(bi, lines.join('\n'));

    text($('summary'), `${entries.length} 件（使用可能 ${state.registered} 件${nf ? `、取れなかったキー ${nf} 件` : ''}）`);
    text($('path'), state.path);
    $('path').title = state.path;
    $('chkAutostart').checked = state.autostart;
  }

  function hasId(issue, id) {
    // 「e03: …」「…: e01, e02」のような文面に、この id が単語として入っているか
    return new RegExp('(^|[^A-Za-z0-9])' + id.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '([^A-Za-z0-9]|$)').test(issue);
  }

  function select(id) {
    selected = id;
    for (const tr of $('rows').children) tr.classList.toggle('sel', tr.dataset.id === id);
    $('btnEdit').disabled = $('btnDelete').disabled = !id;
  }

  async function refresh() {
    state = await invoke('get_state');
    render();
  }

  async function guarded(fn) {
    try { return await fn(); } catch (e) { await info(String(e)); await refresh(); }
  }

  function edit(id) { return guarded(() => invoke('open_edit', { id })); }

  async function setEnabled(id, enabled, cb) {
    try {
      await invoke('set_enabled', { id, enabled });
    } catch (e) {
      cb.checked = !enabled;
      await info(String(e));
    }
  }

  async function del() {
    const e = state.entries.find((x) => x.id === selected);
    if (!e) return;
    if (!(await dialog(`「${e.name}」（${e.hotkey}）を削除しますか？`, { ok: '削除', danger: true }))) return;
    await guarded(() => invoke('delete_entry', { id: e.id }));
    await refresh();
  }

  $('btnAdd').addEventListener('click', () => edit(''));
  $('btnEdit').addEventListener('click', () => selected && edit(selected));
  $('btnDelete').addEventListener('click', del);
  $('btnOpenFile').addEventListener('click', () => guarded(() => invoke('open_config_file')));
  $('btnReload').addEventListener('click', async () => {
    try { await invoke('reload_config'); } catch (e) { /* 失敗は画面上のバナーに出る */ }
    await refresh();
  });
  $('chkAutostart').addEventListener('change', async (ev) => {
    try { await invoke('set_autostart', { enable: ev.target.checked }); } catch (e) { await info(String(e)); }
    await refresh();
  });
  document.addEventListener('keydown', (ev) => {
    if (ev.target.tagName === 'INPUT' && ev.target.type !== 'checkbox') return;
    if (ev.key === 'Delete' && selected) { ev.preventDefault(); del(); }
    else if (ev.key === 'Enter' && selected) { ev.preventDefault(); edit(selected); }
    else if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
      const ids = state.entries.map((x) => x.id);
      if (!ids.length) return;
      ev.preventDefault();
      const i = ids.indexOf(selected);
      const next = ev.key === 'ArrowDown' ? Math.min(ids.length - 1, i + 1) : Math.max(0, i < 0 ? 0 : i - 1);
      select(ids[next]);
    }
  });

  const ev = window.__TAURI__.event;
  if (ev && ev.listen) ev.listen('config-changed', refresh);
  refresh();
})();
