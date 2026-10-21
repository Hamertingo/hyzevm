// HyzeVM — grade das máquinas, foco numa delas e ⌘K para todo o resto.
import { openScreen } from "./screen.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (s, el = document) => el.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const norm = (s) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
const nf = (n, d = 1) => Number(n).toLocaleString("pt-BR", { maximumFractionDigits: d });

/* ------------------------------------------------------------------ ícones */
const P = {
  monitor: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M9 20h6M12 16v4"/>',
  term: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9.5l3 2.5-3 2.5M12.5 15h4.5"/>',
  code: '<path d="M8 8l-4 4 4 4M16 8l4 4-4 4M13.5 5l-3 14"/>',
  plus: '<path d="M12 5v14M5 12h14"/>', back: '<path d="M14 6l-6 6 6 6"/>', x: '<path d="M7 7l10 10M17 7L7 17"/>',
  pause: '<path d="M9 6v12M15 6v12"/>', play: '<path d="M8 5.5v13l10.5-6.5L8 5.5z"/>', power: '<path d="M12 3v8M6.4 6.6a8 8 0 1 0 11.2 0"/>',
  camera: '<path d="M4 8.5A2.5 2.5 0 0 1 6.5 6H8l1.2-1.6h5.6L16 6h1.5A2.5 2.5 0 0 1 20 8.5v8A2.5 2.5 0 0 1 17.5 19h-11A2.5 2.5 0 0 1 4 16.5v-8z"/><circle cx="12" cy="12.4" r="3.4"/>',
  hand: '<path d="M8 13V6.5a1.5 1.5 0 0 1 3 0V12M11 11V5a1.5 1.5 0 0 1 3 0v6M14 11V6.5a1.5 1.5 0 0 1 3 0V14a6 6 0 0 1-6 6h-.5a6 6 0 0 1-4.9-2.6L3.8 14a1.5 1.5 0 0 1 2.4-1.8L8 14"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 7.5h.01"/>', search: '<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-4.2-4.2"/>',
  expand: '<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>',
  copy: '<rect x="8.5" y="8.5" width="11" height="11" rx="2"/><path d="M15.5 8.5V6.5a2 2 0 0 0-2-2h-7a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h2"/>',
  keys: '<rect x="2.8" y="6" width="18.4" height="12" rx="2"/><path d="M6.5 10h.01M10 10h.01M13.5 10h.01M17 10h.01M7 14h10"/>',
  trash: '<path d="M4 7h16M9 7V4.5h6V7M6.5 7l1 13h9l1-13"/>', clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7.5V12l3 2"/>',
  down: '<path d="M12 4v11M7 11l5 5 5-5M5 20h14"/>',
  cursor: '<path d="M6 4l12 7-5.3 1.4L10 18 6 4z"/>', check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>', refresh: '<path d="M20 11a8 8 0 1 0-2.3 5.6M20 4v7h-7"/>',
};
const ic = (n, s = 16, w = 1.6) => `<svg viewBox="0 0 24 24" width="${s}" height="${s}" fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${P[n]}</svg>`;

const TPL = { minimal: ["Terminal", "term", "Debian puro · ~10 s"], base: ["Desenvolvimento", "code", "Docker, Node, Python · ~20 s"], desktop: ["Com tela", "monitor", "navegador e dock · ~20 s"] };
const SIZES = [["leve", "Leve", 2, 2], ["padrao", "Padrão", 4, 4], ["forte", "Forte", 8, 12]];
const DISKS = [16, 32, 64, 128];
const IDLE = [[0, "Nunca"], [600, "10 min"], [1800, "30 min"], [3600, "1 hora"], [14400, "4 horas"]];

/* ------------------------------------------------------------------ estado */
const S = {
  update: null, machines: [], sig: "", view: "grid", sel: null, control: false, info: false, k: null,
  thumbs: {}, stats: {}, cpu: {}, creating: null, screen: null, consoles: {}, busy: {},
};
const cur = () => S.machines.find((m) => m.name === S.sel);
const fmtUp = (s) => (s >= 86400 ? `${Math.floor(s / 86400)} d ${Math.floor((s % 86400) / 3600)} h` : s >= 3600 ? `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min` : `${Math.max(1, Math.floor(s / 60))} min`);
const dotOf = (m) => (m.takeover ? "wait" : m.state === "running" ? "running" : "");
function subOf(m) {
  if (S.creating?.name === m.name) return S.creating.step;
  if (m.takeover) return m.takeover;
  if (S.busy[m.name]) return S.busy[m.name];
  if (m.state === "running") { const st = S.stats[m.name]; return (st ? `ligada há ${fmtUp(st.uptime_secs)}` : "ligada") + (m.always_on ? " · 24 h" : ""); }
  return m.state === "paused" ? "pausada · retoma ao abrir" : "desligada";
}

/* ------------------------------------------------------------------ toast */
let toastT;
function toast(html, { err = false, sticky = false } = {}) {
  const t = $("#toast");
  t.innerHTML = `<div class="toast ${err ? "err" : ""}">${html}</div>`;
  clearTimeout(toastT);
  if (!sticky) toastT = setTimeout(() => (t.innerHTML = ""), err ? 5200 : 2400);
}
const call = async (cmd, args) => { try { return await invoke(cmd, args); } catch (e) { toast(esc(e), { err: true }); throw e; } };

/* ================================================================== atualização */
const updPill = () => (S.update ? `<button class="updpill" data-act="upd" title="Nova versão disponível">${ic("down", 13, 2)}v${esc(S.update.version)}</button>` : "");
async function checkUpdate(force = false) {
  try {
    const u = await invoke("update_check", { force });
    S.update = u; renderTop();
    if (force) { if (u) openUpd(); else toast("Você já está na versão mais recente"); }
  } catch (e) { if (force) toast(esc(e), { err: true }); }
}
function openUpd(state = "ask") {
  const u = S.update; if (!u) return;
  const notes = u.notes.split("\n").map((l) => l.replace(/^[-*#\s]+/, "").trim()).filter(Boolean).slice(0, 5).map((l) => `<li>${esc(l)}</li>`).join("");
  $("#upd").innerHTML = `<div class="kscrim" data-updclose><div class="updbox">
    <div class="uh"><span class="ui">${ic("down", 20, 1.8)}</span><div><b>Nova versão disponível</b><span>v${esc(u.current)} → <i>v${esc(u.version)}</i></span></div></div>
    ${notes ? `<ul>${notes}</ul>` : ""}
    ${state === "busy" ? `<div class="ubusy"><span class="spin"></span>Baixando e verificando…</div>`
      : `<div class="uf"><button class="pill soft" data-act="updno">Depois</button>${u.can_install ? `<button class="pill solid" data-act="updgo">Atualizar e reiniciar</button>` : `<button class="pill solid" data-act="updpage">Abrir página de download</button>`}</div>`}
  </div></div>`;
}
const closeUpd = () => ($("#upd").innerHTML = "");
async function installUpd() {
  openUpd("busy");
  try { await invoke("update_install"); }                       // o app fecha e o novo abre sozinho
  catch (e) { openUpd(); toast(esc(e), { err: true }); }
}

/* ================================================================== topo */
function renderTop() {
  const top = $("#top");
  if (S.view !== "focus" || !cur()) {
    top.innerHTML = `<button class="find" data-act="k">${ic("search", 14, 1.8)}<span>Buscar ou executar…</span><span class="keys"><i>⌘</i><i>K</i></span></button>
      <div class="grow" data-tauri-drag-region></div>${updPill()}<button class="ib" title="Nova máquina  ⌘N" data-act="new">${ic("plus", 17)}</button>`;
    return;
  }
  const m = cur(), desk = m.has_screen && m.state === "running";
  const status = m.takeover && !S.control ? `<span class="dot wait"></span><span>precisa de você · ${esc(m.takeover)}</span>`
    : S.control ? `<span class="dot you"></span><span>você está no controle</span>`
    : `<span class="dot ${dotOf(m)}"></span><span>${esc(subOf(m))}</span>`;
  const pill = !desk ? "" : S.control ? `<button class="pill blue" data-act="give">Devolver ao agente</button>`
    : `<button class="pill ${m.takeover ? "solid" : "soft"}" data-act="take" title="⌘↩">${ic("hand", 13)}Assumir</button>`;
  const pp = m.state === "running" ? `<button class="ib" title="Pausar" data-act="suspend">${ic("pause", 15, 1.9)}</button>` : `<button class="ib" title="Ligar" data-act="start">${ic("play", 14, 1.8)}</button>`;
  top.innerHTML = `<button class="back" data-act="home" title="Voltar  esc">${ic("back", 15, 1.8)}<b>${esc(m.name)}</b></button><span class="status">${status}</span>
    <div class="grow" data-tauri-drag-region></div>${updPill()}${pill}${pp}<button class="ib" title="Informações  ⌘I" data-act="info">${ic("info", 16)}</button>`;
}

/* ================================================================== grade */
function tileScreen(m) {
  if (!m.has_screen) {
    const st = S.stats[m.name];
    return `<div class="termcard"><div><span class="g">agent@${esc(m.name)}</span> <span class="d">· Debian 12</span></div>
      ${st ? `<div class="d">ligada há ${fmtUp(st.uptime_secs)}</div><div>mem <span class="ok">${nf(st.mem_used_mb / 1024)} GB</span> <span class="d">de ${nf(st.mem_total_mb / 1024)} GB</span></div><div>disco <span class="ok">${nf(st.disk_used_gb)} GB</span> <span class="d">de ${nf(st.disk_total_gb, 0)} GB</span></div>`
        : `<div class="d">${m.state === "running" ? "…" : m.state === "paused" ? "pausada" : "desligada"}</div>`}
      <div><span class="g">$</span> ▍</div></div>`;
  }
  const t = S.thumbs[m.name];
  const label = m.state === "paused" ? "Pausada" : m.state === "stopped" ? "Desligada" : t ? "" : "Carregando a tela…";
  return `${t ? `<img alt="" src="${t}">` : ""}${label ? `<div class="ph">${label}</div>` : ""}`;
}
function renderGrid() {
  const main = $("#main");
  if (!S.machines.length && !S.creating) {
    main.innerHTML = `<div class="empty"><div><h2>Nenhuma máquina ainda</h2><p>Aperte <span class="kbd">⌘</span><span class="kbd">K</span> e digite um nome.</p><br>
      <button class="pill solid" data-act="new">${ic("plus", 13, 2)}Nova máquina</button></div></div>`;
    return;
  }
  const tiles = S.machines.map((m, i) => `<div class="tile ${m.takeover ? "wait" : ""} ${m.state}" data-open="${esc(m.name)}" data-name="${esc(m.name)}" title="${esc(m.name)}  ⌘${i + 1}">
      <div class="scr">${tileScreen(m)}${m.takeover ? `<span class="flag">${ic("hand", 12, 2)}Precisa de você</span>` : ""}
        <div class="acts">${m.state === "running" ? `<button title="Pausar" data-tact="suspend">${ic("pause", 13, 2)}</button>` : `<button title="Ligar" data-tact="start">${ic("play", 13, 2)}</button>`}<button title="Abrir">${ic("expand", 13, 1.8)}</button></div></div>
      <div class="meta"><span class="dot ${dotOf(m)}"></span><b>${esc(m.name)}</b><span>${esc(subOf(m))}</span></div></div>`).join("");
  const creating = S.creating && !S.machines.some((m) => m.name === S.creating.name)
    ? `<div class="tile creating"><div class="scr"><div><span class="spin"></span><span id="creating-step">${esc(S.creating.step)}</span></div></div><div class="meta"><span class="dot"></span><b>${esc(S.creating.name)}</b><span>criando</span></div></div>` : "";
  main.innerHTML = `<div class="gridwrap"><div class="grid">${tiles}${creating}</div></div>`;
}
function paintTile(name) {
  const m = S.machines.find((x) => x.name === name), el = document.querySelector(`.tile[data-name="${CSS.escape(name)}"]`);
  if (!m || !el) return;
  const scr = el.querySelector(".scr"), acts = scr.querySelector(".acts"), flag = scr.querySelector(".flag");
  scr.innerHTML = tileScreen(m);
  if (flag) scr.append(flag);
  scr.append(acts);
  el.querySelector(".meta span:last-child").textContent = subOf(m);
}

/* ================================================================== foco */
function dropScreen() {
  const sc = S.screen; if (!sc) return;
  S.screen = null; const r = sc.rfb; sc.rfb = null;
  try { r?.disconnect(); } catch {}
  sc.host.remove();
}
async function connectScreen(m, sc) {
  sc.state = "connecting"; paintVeil();
  try {
    const url = await invoke("screen_ws", { name: m.name });
    if (S.screen !== sc) return;
    sc.rfb = openScreen(sc.host, url, {
      connect: () => { sc.state = "live"; paintVeil(); },
      disconnect: () => { if (sc.rfb) { sc.rfb = null; sc.state = "down"; paintVeil(); } },
    });
    sc.rfb.viewOnly = !S.control;
  } catch (e) { sc.state = "down"; paintVeil(); toast(esc(e), { err: true }); }
}
function paintVeil() {
  const v = $("#veil"), sc = S.screen; if (!v || !sc) return;
  if (sc.state === "live") { v.innerHTML = ""; return; }
  v.innerHTML = sc.state === "down"
    ? `<div class="veil"><div><b>Conexão perdida</b><p>A tela parou de responder.</p><button class="pill solid" data-act="reconnect">${ic("refresh", 13, 1.8)}Reconectar</button></div></div>`
    : `<div class="veil"><div><span class="spin"></span>Conectando…</div></div>`;
}
function renderFocus() {
  const m = cur(), main = $("#main");
  if (!m) { S.view = "grid"; return renderGrid(); }
  const ring = S.control ? "you" : m.takeover ? "wait" : "";
  if (!m.has_screen) {
    dropScreen();
    main.innerHTML = `<div class="focus"><div class="scr ${ring}" id="fscr"></div></div>`;
    $("#fscr").append(consoleFor(m));
    setTimeout(() => $("#fscr input")?.focus(), 0);
    return;
  }
  if (m.state !== "running") {
    dropScreen();
    const t = S.thumbs[m.name];
    const busy = S.busy[m.name];
    main.innerHTML = `<div class="focus"><div class="scr">${t ? `<img class="thumbbg" alt="" src="${t}">` : ""}<div class="veil"><div>
      ${busy ? `<span class="spin"></span>${esc(busy)}` : `<b>${m.state === "paused" ? "Pausada" : "Desligada"}</b><p>${m.state === "paused" ? "Memória e processos guardados. Volta de onde parou." : "Ligue para ver a tela."}</p><button class="pill solid" data-act="start">${ic("play", 12, 2)}${m.state === "paused" ? "Retomar" : "Ligar"}</button>`}</div></div></div></div>`;
    return;
  }
  let sc = S.screen;
  if (sc && sc.name !== m.name) { dropScreen(); sc = null; }
  const fresh = !sc;
  if (fresh) { const host = document.createElement("div"); host.className = "screenhost"; sc = S.screen = { name: m.name, host, rfb: null, state: "connecting" }; }
  main.innerHTML = `<div class="focus"><div class="scr ${ring}" id="fscr">${S.control ? "" : `<div class="shield" data-act="nudge"></div>`}<div id="veil"></div>
    ${S.control ? `<div class="hint">Teclado e mouse na máquina<span class="kbd">⌘⎋</span>devolve</div>` : ""}</div></div>`;
  $("#fscr").prepend(sc.host);
  if (fresh) connectScreen(m, sc); else paintVeil();
  if (sc.rfb) { sc.rfb.viewOnly = !S.control; if (S.control) setTimeout(() => sc.rfb?.focus?.(), 50); }
}
function consoleFor(m) {
  let c = S.consoles[m.name];
  if (c) return c.root;
  const root = document.createElement("div"); root.className = "console";
  root.innerHTML = `<div class="out"><div class="d">agent@${esc(m.name)} · os comandos rodam dentro da máquina</div></div>
    <form><b>$</b><input autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="digite um comando"></form>`;
  c = S.consoles[m.name] = { root, hist: [], pos: 0 };
  const out = $(".out", root), input = $("input", root);
  $("form", root).onsubmit = async (e) => {
    e.preventDefault();
    const cmd = input.value.trim(); if (!cmd) return;
    input.value = ""; c.hist.push(cmd); c.pos = c.hist.length;
    if (cmd === "clear") { out.innerHTML = ""; return; }
    out.insertAdjacentHTML("beforeend", `<div class="c">${esc(cmd)}</div>`);
    out.scrollTop = out.scrollHeight;
    try {
      const r = await invoke("exec", { name: m.name, command: cmd });
      if (r.stdout) out.insertAdjacentHTML("beforeend", `<div>${esc(r.stdout.replace(/\n$/, ""))}</div>`);
      if (r.stderr) out.insertAdjacentHTML("beforeend", `<div class="e">${esc(r.stderr.replace(/\n$/, ""))}</div>`);
      if (r.code !== 0) out.insertAdjacentHTML("beforeend", `<div class="d">código ${r.code}</div>`);
    } catch (err) { out.insertAdjacentHTML("beforeend", `<div class="e">${esc(err)}</div>`); }
    out.scrollTop = out.scrollHeight;
  };
  input.onkeydown = (e) => {
    if (e.key === "ArrowUp" && c.pos > 0) { input.value = c.hist[--c.pos]; e.preventDefault(); }
    if (e.key === "ArrowDown") { c.pos = Math.min(c.pos + 1, c.hist.length); input.value = c.hist[c.pos] || ""; e.preventDefault(); }
  };
  return root;
}

/* ================================================================== informações (⌘I) */
function renderInfo() {
  const box = $("#info"), m = cur();
  if (!S.info || S.view !== "focus" || !m) { box.innerHTML = ""; return; }
  const st = S.stats[m.name], run = m.state === "running";
  const bar = (id, p) => `<span class="bar"><i id="${id}" class="${p > 80 ? "warn" : ""}" style="width:${p || 0}%"></i></span>`;
  const cpu = S.cpu[m.name] ?? null;
  box.innerHTML = `<div class="info">
    <div class="r"><span>CPU</span>${bar("ib-cpu", cpu)}<span class="v" id="iv-cpu">${run && cpu != null ? Math.round(cpu) + "%" : "—"}</span></div>
    <div class="r"><span>Memória</span>${bar("ib-mem", st ? (st.mem_used_mb / st.mem_total_mb) * 100 : 0)}<span class="v" id="iv-mem">${run && st ? nf(st.mem_used_mb / 1024) + " GB" : "—"}</span></div>
    <div class="r"><span>Disco</span>${bar("ib-dsk", st ? (st.disk_used_gb / st.disk_total_gb) * 100 : 0)}<span class="v" id="iv-dsk">${st ? nf(st.disk_used_gb) + " GB" : "—"}</span></div><hr>
    <div class="r"><span>Recursos</span><span>${m.cpus} CPU · ${nf(m.memory_mb / 1024, 0)} GB · ${m.disk_gb} GB</span></div>
    <div class="r"><span>Ligada há</span><span id="iv-up">${run && st ? fmtUp(st.uptime_secs) : "—"}</span></div>
    <div class="r"><span>Ligada 24 h</span><button class="sw ${m.always_on ? "on" : ""}" data-act="always" aria-label="Ligada 24 horas"></button></div>
    <div class="r"><span>Pausar se ociosa</span><select id="idle" ${m.always_on ? "disabled" : ""}>${m.always_on ? "<option>Nunca</option>" : IDLE.map(([s, l]) => `<option value="${s}" ${s === m.auto_suspend_secs ? "selected" : ""}>${l}</option>`).join("")}</select></div><hr>
    <div class="r"><span class="m">hyzevm ssh ${esc(m.name)}</span><button class="ib" style="width:22px;height:22px" title="Copiar" data-copy="hyzevm ssh ${esc(m.name)}">${ic("copy", 13)}</button></div>
    <div class="r"><span>Terminal</span><button class="ib" style="width:22px;height:22px" title="Abrir no Terminal" data-act="terminal">${ic("term", 14)}</button></div></div>`;
  const sel = $("#idle"); if (sel && !m.always_on) sel.onchange = async () => { await call("set_auto_suspend", { name: m.name, secs: +sel.value }); refresh(true); };
}
function paintInfoStats(m) {
  if (!S.info || S.sel !== m.name || !$("#iv-mem")) return;
  const st = S.stats[m.name]; if (!st) return;
  const set = (id, p) => { const el = $("#" + id); if (el) { el.style.width = Math.min(100, p) + "%"; el.className = p > 80 ? "warn" : ""; } };
  const cpu = S.cpu[m.name];
  if (cpu != null) { set("ib-cpu", cpu); $("#iv-cpu").textContent = Math.round(cpu) + "%"; }
  set("ib-mem", (st.mem_used_mb / st.mem_total_mb) * 100); $("#iv-mem").textContent = nf(st.mem_used_mb / 1024) + " GB";
  set("ib-dsk", (st.disk_used_gb / st.disk_total_gb) * 100); $("#iv-dsk").textContent = nf(st.disk_used_gb) + " GB";
  $("#iv-up").textContent = fmtUp(st.uptime_secs);
}

/* ================================================================== ⌘K */
function suggestName() {
  const have = new Set(S.machines.map((m) => m.name));
  if (!have.has("agente")) return "agente";
  for (let i = 2; i < 999; i++) if (!have.has("agente-" + i)) return "agente-" + i;
  return "";
}
function openK(mode = "main", extra = {}) {
  S.k = { mode, q: "", hl: 0, tpl: "desktop", size: "padrao", disk: 32, always: true, ...extra };
  if (mode === "new") S.k.q = suggestName();
  renderK();
}
function closeK() { S.k = null; $("#k").innerHTML = ""; }

function kItems() {
  const k = S.k, q = norm(k.q.trim()), m = S.view === "focus" ? cur() : null;
  if (k.mode === "new") return Object.entries(TPL).reverse().map(([id, [t, icn, d]]) => ({ id, icon: icn, title: t, sub: d, run: () => createFromK(id) }));
  if (k.mode === "type") return [{ icon: "keys", title: k.q ? `Digitar “${k.q.length > 40 ? k.q.slice(0, 40) + "…" : k.q}”` : "Escreva o texto acima", sub: `em ${k.target}`, run: () => k.q && typeText(k.target, k.q) }];
  if (k.mode === "remove") return [
    { icon: "trash", title: `Remover ${k.target}`, sub: "apaga o disco e tudo dentro dele", danger: true, run: () => removeMachine(k.target) },
    { icon: "x", title: "Cancelar", run: () => openK("main") }];

  const out = [];
  const act = (icon, title, sub, run, kbd, extra = {}) => out.push({ group: "Ações", icon, title, sub, run, kbd, ...extra });
  if (m) {
    if (m.has_screen && m.state === "running") {
      if (S.control) act("hand", "Devolver ao agente", "", () => give(), "⌘⎋");
      else act("hand", "Assumir o controle", m.takeover || "", () => take(), "⌘↩");
      act("camera", "Capturar a tela", "salva em Downloads", () => capture(), "⌘⇧S");
      act("keys", "Digitar texto…", "como se fosse o teclado", () => openK("type", { target: m.name }));
    }
    if (m.state === "running") act("pause", `Pausar ${m.name}`, "guarda memória e processos", () => machineAction(m.name, "suspend"));
    else act("play", `${m.state === "paused" ? "Retomar" : "Ligar"} ${m.name}`, "", () => machineAction(m.name, "start"));
    if (m.state !== "stopped") act("power", `Desligar ${m.name}`, "", () => machineAction(m.name, "stop"));
    act("term", "Abrir no Terminal", "ssh", () => call("open_terminal", { name: m.name }));
    act("info", "Informações", "uso, 24 h, acesso", () => { S.info = true; renderInfo(); }, "⌘I");
    act("clock", m.always_on ? "Desligar o modo 24 h" : "Manter ligada 24 h", "", () => toggleAlways(m));
    act("trash", `Remover ${m.name}…`, "", () => openK("remove", { target: m.name }), "", { danger: true, keep: true });
  }
  for (const x of S.machines.filter((x) => x.takeover && x !== m)) act("hand", `Assumir ${x.name}`, x.takeover, () => { openMachine(x.name); take(); });
  act("plus", "Nova máquina", "", () => openK("new"), "⌘N", { keep: true });
  act("down", S.update ? `Atualizar para v${S.update.version}` : "Verificar atualizações", S.update ? "nova versão disponível" : "", () => (S.update ? openUpd() : checkUpdate(true)));
  if (S.view === "focus") act("back", "Voltar para a grade", "", () => goHome(), "esc");
  if (q) {
    for (const x of S.machines) {
      if (x === m) continue;
      if (x.state === "running") act("pause", `Pausar ${x.name}`, "", () => machineAction(x.name, "suspend"));
      else act("play", `${x.state === "paused" ? "Retomar" : "Ligar"} ${x.name}`, "", () => machineAction(x.name, "start"));
      act("trash", `Remover ${x.name}…`, "", () => openK("remove", { target: x.name }), "", { danger: true, keep: true });
    }
  }
  S.machines.forEach((x, i) => out.push({ group: "Máquinas", thumb: x, title: x.name, sub: subOf(x), kbd: i < 9 ? "⌘" + (i + 1) : "", run: () => openMachine(x.name) }));
  if (!q) return out;
  return out.filter((it) => norm(`${it.title} ${it.sub || ""}`).includes(q));
}

function renderK() {
  const box = $("#k"), k = S.k;
  if (!k) { box.innerHTML = ""; return; }
  const ph = { main: "Buscar ou executar", new: "Nome da máquina", type: "Texto para digitar", remove: "" }[k.mode];
  const crumb = { new: "Nova máquina", type: "Digitar", remove: "Remover" }[k.mode];
  const size = SIZES.find((s) => s[0] === k.size);
  box.innerHTML = `<div class="kscrim" data-kclose><div class="kbox">
    <div class="kq">${ic(k.mode === "new" ? "plus" : k.mode === "type" ? "keys" : k.mode === "remove" ? "trash" : "search", 18)}${crumb ? `<span class="crumb">${crumb}</span>` : ""}
      <input id="kin" placeholder="${ph}" value="${esc(k.q)}" ${k.mode === "remove" ? "readonly" : ""} spellcheck="false" autocomplete="off"></div>
    <div class="klist" id="klist"></div>
    ${k.mode === "new" ? `<div class="kopts"><button class="chip" data-kopt="size">Tamanho <b>${size[1]}</b> · ${size[2]} CPU · ${size[3]} GB</button><button class="chip" data-kopt="disk">Disco <b>${k.disk} GB</b></button><button class="chip" data-kopt="always">Ligada 24 h <b>${k.always ? "sim" : "não"}</b></button></div>` : ""}
    <div class="kfoot">${k.mode === "new" ? `<span><span class="kbd">↑</span><span class="kbd">↓</span> modelo</span><span><span class="kbd">⇥</span> tamanho</span><span class="grow"></span><span><span class="kbd">↵</span> criar</span><span><span class="kbd">esc</span> voltar</span>`
      : `<span><span class="kbd">↑</span><span class="kbd">↓</span> navegar</span><span class="grow"></span><span><span class="kbd">↵</span> ${k.mode === "main" ? "executar" : "confirmar"}</span><span><span class="kbd">esc</span> ${k.mode === "main" ? "fechar" : "voltar"}</span>`}</div></div></div>`;
  const input = $("#kin");
  input.focus(); input.setSelectionRange(input.value.length, input.value.length);
  input.oninput = () => { k.q = input.value; if (k.mode !== "new") k.hl = 0; renderKList(); };
  renderKList();
}
function renderKList() {
  const k = S.k, list = $("#klist"); if (!k || !list) return;
  const items = kItems();
  k.items = items;
  if (k.mode === "new") k.hl = Math.max(0, Object.keys(TPL).reverse().indexOf(k.tpl));
  k.hl = Math.min(k.hl, Math.max(0, items.length - 1));
  if (!items.length) { list.innerHTML = `<div class="kempty">Nada encontrado.</div>`; return; }
  let html = "", g = null;
  items.forEach((it, i) => {
    const grp = k.mode === "new" ? "Modelo" : it.group;
    if (grp && grp !== g) { html += `<small>${grp}</small>`; g = grp; }
    const lead = it.thumb ? `<span class="mt">${it.thumb.has_screen && S.thumbs[it.thumb.name] ? `<img alt="" src="${S.thumbs[it.thumb.name]}">` : ic(it.thumb.has_screen ? "monitor" : "term", 11)}</span>` : ic(it.icon, 15);
    html += `<div class="ko ${i === k.hl ? "hl" : ""} ${it.danger ? "danger" : ""}" data-ki="${i}">${lead}<span class="g">${esc(it.title)}${it.sub ? `<em>${esc(it.sub)}</em>` : ""}</span>${it.kbd ? `<span class="kbd">${it.kbd}</span>` : i === k.hl ? '<span class="kbd">↵</span>' : ""}</div>`;
  });
  list.innerHTML = html;
  list.querySelector(".ko.hl")?.scrollIntoView({ block: "nearest" });
}
function kRun(i) {
  const k = S.k, it = k?.items?.[i]; if (!it) return;
  if (!it.keep) closeK();
  it.run();
}
function kMove(d) {
  const k = S.k; if (!k?.items?.length) return;
  if (k.mode === "new") { const ids = Object.keys(TPL).reverse(); k.tpl = ids[(ids.indexOf(k.tpl) + d + ids.length) % ids.length]; }
  else k.hl = (k.hl + d + k.items.length) % k.items.length;
  renderKList();
}
document.addEventListener("click", (e) => {
  if (!S.k) return;
  const o = e.target.closest("[data-ki]");
  if (o) { if (S.k.mode === "new") { S.k.tpl = S.k.items[+o.dataset.ki].id; return createFromK(S.k.tpl); } return kRun(+o.dataset.ki); }
  const opt = e.target.closest("[data-kopt]");
  if (opt) {
    const k = S.k;
    if (opt.dataset.kopt === "size") k.size = SIZES[(SIZES.findIndex((s) => s[0] === k.size) + 1) % SIZES.length][0];
    if (opt.dataset.kopt === "disk") k.disk = DISKS[(DISKS.indexOf(k.disk) + 1) % DISKS.length];
    if (opt.dataset.kopt === "always") k.always = !k.always;
    return renderK();
  }
  if (e.target.matches("[data-kclose]")) closeK();
});
document.addEventListener("mousemove", (e) => {
  const o = e.target.closest?.("[data-ki]"); if (!o || !S.k || S.k.mode === "new") return;
  const i = +o.dataset.ki; if (i !== S.k.hl) { S.k.hl = i; document.querySelectorAll(".ko").forEach((x, j) => x.classList.toggle("hl", j === i)); }
});

/* ================================================================== ações */
async function refresh(force = false) {
  try {
    const ms = await invoke("list_machines");
    const sig = JSON.stringify(ms);
    S.machines = ms;
    if (S.view === "focus" && !cur()) { S.view = "grid"; S.control = false; dropScreen(); force = true; }
    if (force || sig !== S.sig) { S.sig = sig; render(); if (S.k?.mode === "main") renderKList(); }
  } catch { /* o app ainda está subindo */ }
}
function render() { renderTop(); S.view === "focus" ? renderFocus() : renderGrid(); renderInfo(); renderAsks(); }

/* perguntas do agente (ask): cartão fixo; só redesenha quando a lista muda, para não perder o texto digitado */
let asksSig = "";
function renderAsks() {
  const all = S.machines.flatMap((m) => (m.asks || []).map((a) => ({ ...a, name: m.name })));
  const sig = JSON.stringify(all);
  if (sig === asksSig) return;
  asksSig = sig;
  $("#asks").innerHTML = all.slice(0, 3).map((a) => `<div class="ask" data-n="${esc(a.name)}" data-id="${esc(a.id)}">
      <div class="q"><span class="dot wait"></span><b>${esc(a.name)}</b> pergunta</div><p>${esc(a.question)}</p>
      ${a.options.length ? `<div class="opts">${a.options.map((o) => `<button class="pill soft" data-ans="${esc(o)}">${esc(o)}</button>`).join("")}</div>`
        : `<form class="reply"><input placeholder="Sua resposta…" autocomplete="off"><button class="pill solid">Enviar</button></form>`}</div>`).join("");
}
async function sendAnswer(card, text) {
  if (!text.trim()) return;
  try { await call("ask_answer", { name: card.dataset.n, id: card.dataset.id, text }); toast("Resposta enviada ao agente"); } catch {}
  refresh(true);
}
$("#asks").addEventListener("click", (e) => { const b = e.target.closest("[data-ans]"); if (b) sendAnswer(b.closest(".ask"), b.dataset.ans); });
$("#asks").addEventListener("submit", (e) => { e.preventDefault(); const f = e.target; sendAnswer(f.closest(".ask"), f.querySelector("input").value); });

async function machineAction(name, action, label) {
  const lbl = label || { start: "ligando…", suspend: "pausando…", stop: "desligando…", force_stop: "desligando…" }[action];
  S.busy[name] = lbl; render();
  try { await call("machine_action", { name, action }); if (action !== "start") delete S.thumbs[name]; }
  catch {} finally { delete S.busy[name]; await refresh(true); }
}
async function openMachine(name) {
  if (S.view === "focus" && S.sel !== name) { dropScreen(); S.control = false; }
  S.sel = name; S.view = "focus"; S.info = false; render();
  const m = cur();
  if (m && m.state !== "running") await machineAction(name, "start", m.state === "paused" ? "retomando…" : "ligando…");
}
function goHome() { dropScreen(); S.view = "grid"; S.control = false; S.info = false; render(); }
function take() {
  const m = cur(); if (!m?.has_screen || m.state !== "running") return;
  S.control = true; render();
}
async function give() {
  const m = cur(); S.control = false; render();
  if (m?.takeover) { try { await call("takeover_done", { name: m.name }); toast("Controle devolvido ao agente"); } catch {} refresh(true); }
}
async function capture() {
  const sc = S.screen; const d = sc?.rfb?.toDataURL?.();
  if (!d) return toast("A tela ainda não está pronta", { err: true });
  try { await call("save_screenshot", { name: sc.name, data_url: d }); toast("Captura salva em Downloads"); } catch {}
}
async function typeText(name, text) { try { await call("desktop_type", { name, text }); toast("Texto digitado"); } catch {} }
async function toggleAlways(m) { try { await call("set_always_on", { name: m.name, on: !m.always_on }); toast(m.always_on ? "Modo 24 h desligado" : "Ligada 24 h: sobe com o computador e volta se cair"); } catch {} refresh(true); }
async function removeMachine(name) {
  if (S.sel === name) goHome();
  S.busy[name] = "removendo…"; render();
  try { await call("machine_action", { name, action: "remove" }); toast(`${esc(name)} removida`); delete S.thumbs[name]; delete S.consoles[name]; } catch {}
  delete S.busy[name]; refresh(true);
}
async function createFromK(tpl) {
  const k = S.k, name = k.q.trim();
  if (!/^[A-Za-z0-9_-]{1,32}$/.test(name)) return toast("Use só letras, números, - e _ no nome.", { err: true });
  if (S.machines.some((m) => m.name === name)) return toast("Já existe uma máquina com esse nome.", { err: true });
  const [, cpus, mem] = SIZES.find((s) => s[0] === k.size).slice(1);
  closeK();
  S.creating = { name, step: "preparando…" };
  if (S.view !== "grid") goHome(); else render();
  toast(`<span class="spin"></span>Criando <b>${esc(name)}</b> · <span id="tstep">preparando…</span>`, { sticky: true });
  try {
    await invoke("create_machine", { req: { name, template: tpl, cpus, memory_gb: mem, disk_gb: k.disk, always_on: k.always } });
    S.creating = null; await refresh(true);
    toast(`<b>${esc(name)}</b> pronta`);
    openMachine(name);
  } catch (e) { S.creating = null; render(); toast(esc(e), { err: true }); }
}
listen("create-progress", (ev) => {
  if (!S.creating || !ev.payload?.msg) return;
  S.creating.step = String(ev.payload.msg).replace(/^\s*\d+s\s+/, "").slice(0, 80);
  const a = $("#creating-step"), b = $("#tstep");
  if (a) a.textContent = S.creating.step; if (b) b.textContent = S.creating.step;
});

/* ================================================================== eventos */
document.addEventListener("click", async (e) => {
  if (e.target.matches?.("[data-updclose]") && !$(".ubusy")) return closeUpd();
  if (S.k) return;
  const cp = e.target.closest("[data-copy]");
  if (cp) { try { await navigator.clipboard.writeText(cp.dataset.copy); toast("Copiado"); } catch { toast("Não foi possível copiar", { err: true }); } return; }
  const ta = e.target.closest("[data-tact]");
  if (ta) { e.stopPropagation(); const name = ta.closest("[data-name]").dataset.name; return machineAction(name, ta.dataset.tact); }
  const a = e.target.closest("[data-act]")?.dataset.act;
  if (a) {
    const m = cur();
    switch (a) {
      case "k": return openK();
      case "upd": return openUpd();
      case "updno": return closeUpd();
      case "updgo": return installUpd();
      case "updpage": closeUpd(); return call("open_url", { url: S.update?.page || "https://github.com/Hamertingo/hyzevm/releases" });
      case "new": return openK("new");
      case "home": return goHome();
      case "take": return take();
      case "give": return give();
      case "info": S.info = !S.info; return renderInfo();
      case "suspend": case "start": return m && machineAction(m.name, a);
      case "always": return m && toggleAlways(m);
      case "terminal": return m && call("open_terminal", { name: m.name });
      case "reconnect": { const sc = S.screen; if (sc) { const r = sc.rfb; sc.rfb = null; try { r?.disconnect(); } catch {} sc.host.replaceChildren(); connectScreen(m, sc); } return; }
      case "nudge": return toast(`Para controlar, clique em <b>Assumir</b> ou aperte <span class="kbd">⌘↩</span>`);
    }
  }
  const tile = e.target.closest("[data-open]");
  if (tile) return openMachine(tile.dataset.open);
  if (S.info && !e.target.closest(".info")) { S.info = false; renderInfo(); }
});

// Captura (true): atalhos com ⌘ funcionam mesmo com o teclado dentro da tela da VM.
addEventListener("keydown", (e) => {
  const cmd = e.metaKey || e.ctrlKey;
  if (S.k) {
    if (e.key === "ArrowDown") { e.preventDefault(); return kMove(1); }
    if (e.key === "ArrowUp") { e.preventDefault(); return kMove(-1); }
    if (e.key === "Tab" && S.k.mode === "new") { e.preventDefault(); S.k.size = SIZES[(SIZES.findIndex((s) => s[0] === S.k.size) + 1) % SIZES.length][0]; return renderK(); }
    if (e.key === "Enter") { e.preventDefault(); return S.k.mode === "new" ? createFromK(S.k.tpl) : kRun(S.k.hl); }
    if (e.key === "Escape") { e.preventDefault(); return S.k.mode === "main" ? closeK() : openK("main"); }
    if (cmd && e.key.toLowerCase() === "k") { e.preventDefault(); return closeK(); }
    return;
  }
  if (cmd && e.key.toLowerCase() === "k") { e.preventDefault(); return openK(); }
  if (S.control && S.view === "focus" && !(cmd && e.key === "Escape") && !(cmd && e.shiftKey && e.key.toLowerCase() === "s")) return; // com o controle, o resto vai para a VM
  if (cmd && e.key.toLowerCase() === "n") { e.preventDefault(); return openK("new"); }
  if (cmd && e.key === "Escape") { e.preventDefault(); return S.control && give(); }
  if (cmd && e.key === "Enter") { e.preventDefault(); return take(); }
  if (cmd && e.key.toLowerCase() === "i" && S.view === "focus") { e.preventDefault(); S.info = !S.info; return renderInfo(); }
  if (cmd && e.shiftKey && e.key.toLowerCase() === "s") { e.preventDefault(); return capture(); }
  if (cmd && /^[1-9]$/.test(e.key)) { const m = S.machines[+e.key - 1]; if (m) { e.preventDefault(); return openMachine(m.name); } }
  if (e.key === "Escape" && !S.control) {
    if (S.info) { S.info = false; return renderInfo(); }
    if (S.view === "focus" && !e.target.closest?.("form")) return goHome();
  }
}, true);

/* ================================================================== botão direito */
const CTX = { items: [], hl: -1 };
function closeCtx() { $("#ctx").innerHTML = ""; CTX.items = []; CTX.hl = -1; }
function ctxFor(m, fromGrid) {
  const it = [], run = m.state === "running", desk = m.has_screen, mine = S.view === "focus" && S.sel === m.name;
  if (fromGrid) it.push({ icon: "expand", title: "Abrir", kbd: "↵", run: () => openMachine(m.name) });
  if (desk && run) {
    if (S.control && mine) it.push({ icon: "hand", title: "Devolver ao agente", kbd: "⌘⎋", run: () => give() });
    else it.push({ icon: "hand", title: m.takeover ? "Assumir (precisa de você)" : "Assumir o controle", kbd: "⌘↩", run: async () => { await openMachine(m.name); take(); } });
    if (mine) it.push({ icon: "camera", title: "Capturar a tela", kbd: "⌘⇧S", run: () => capture() });
    it.push({ icon: "keys", title: "Digitar texto…", run: () => openK("type", { target: m.name }) });
  }
  it.push("-");
  if (run) it.push({ icon: "pause", title: "Pausar", run: () => machineAction(m.name, "suspend") });
  else it.push({ icon: "play", title: m.state === "paused" ? "Retomar" : "Ligar", run: () => machineAction(m.name, "start") });
  if (m.state !== "stopped") it.push({ icon: "power", title: "Desligar", run: () => machineAction(m.name, "stop") });
  it.push("-");
  it.push({ icon: "term", title: "Abrir no Terminal", run: () => call("open_terminal", { name: m.name }) });
  it.push({ icon: "info", title: "Informações", kbd: "⌘I", run: async () => { await openMachine(m.name); S.info = true; renderInfo(); } });
  it.push({ icon: "clock", title: "Manter ligada 24 h", check: m.always_on, run: () => toggleAlways(m) });
  it.push({ icon: "copy", title: "Copiar comando SSH", run: () => navigator.clipboard.writeText(`hyzevm ssh ${m.name}`).then(() => toast("Copiado"), () => toast("Não foi possível copiar", { err: true })) });
  it.push("-");
  it.push({ icon: "trash", title: "Remover…", danger: true, run: () => openK("remove", { target: m.name }) });
  return it;
}
function showCtx(x, y, items, m) {
  CTX.items = items.filter((i) => i !== "-"); CTX.hl = -1;
  let n = -1;
  const rows = items.map((i) => i === "-" ? '<hr>' : `<div class="ci ${i.danger ? "danger" : ""}" data-ci="${++n}">${ic(i.icon, 14)}<span class="t">${esc(i.title)}</span>${i.check ? ic("check", 13, 2) : ""}${i.kbd ? `<span class="sc">${i.kbd}</span>` : ""}</div>`).join("");
  const head = m ? `<div class="chd"><span class="dot ${dotOf(m)}"></span><b>${esc(m.name)}</b><span>${esc(subOf(m))}</span></div>` : "";
  $("#ctx").innerHTML = `<div class="ctx" style="left:${x}px;top:${y}px">${head}${rows}</div>`;
  const el = $("#ctx .ctx"), r = el.getBoundingClientRect();
  if (r.right > innerWidth - 6) el.style.left = Math.max(6, innerWidth - r.width - 6) + "px";
  if (r.bottom > innerHeight - 6) el.style.top = Math.max(6, innerHeight - r.height - 6) + "px";
}
function ctxHl(i) { CTX.hl = i; document.querySelectorAll("#ctx .ci").forEach((x, j) => x.classList.toggle("hl", j === i)); }
document.addEventListener("contextmenu", (e) => {
  if (S.control && e.target.closest("#fscr")) return;              // no controle, o clique direito vai para a VM
  if (e.target.closest("input, textarea, .console .out")) return;   // menu nativo de texto
  e.preventDefault();
  if (S.k) return;
  const tile = e.target.closest(".tile[data-name]");
  const m = tile ? S.machines.find((x) => x.name === tile.dataset.name) : S.view === "focus" && e.target.closest("#fscr, .top") ? cur() : null;
  if (m) return showCtx(e.clientX, e.clientY, ctxFor(m, !!tile), m);
  if (S.view === "grid") return showCtx(e.clientX, e.clientY, [{ icon: "plus", title: "Nova máquina", kbd: "⌘N", run: () => openK("new") }]);
  closeCtx();
}, true);
document.addEventListener("mousedown", (e) => {
  if (!$("#ctx").innerHTML) return;
  const ci = e.target.closest("[data-ci]");
  if (!ci) { closeCtx(); if (e.target.closest(".tile, [data-act], [data-open]")) e.stopPropagation(); return; }
}, true);
document.addEventListener("click", (e) => {
  const ci = e.target.closest("[data-ci]"); if (!ci) return;
  e.stopPropagation();
  const it = CTX.items[+ci.dataset.ci]; closeCtx(); it?.run();
}, true);
document.addEventListener("mouseover", (e) => { const ci = e.target.closest?.("[data-ci]"); if (ci) ctxHl(+ci.dataset.ci); });
addEventListener("keydown", (e) => {
  if (!$("#ctx").innerHTML) return;
  e.stopPropagation(); e.preventDefault();
  if (e.key === "Escape") return closeCtx();
  if (e.key === "ArrowDown") return ctxHl((CTX.hl + 1) % CTX.items.length);
  if (e.key === "ArrowUp") return ctxHl((CTX.hl - 1 + CTX.items.length) % CTX.items.length);
  if (e.key === "Enter" && CTX.hl >= 0) { const it = CTX.items[CTX.hl]; closeCtx(); it.run(); }
}, true);
addEventListener("blur", closeCtx);

/* ================================================================== teclado da VM */
// O Mac manda ⌘ como "Super" para o Linux e o noVNC trata Caps Lock à parte; aqui o teclado
// do Mac vira o do Linux: ⌘ = Ctrl, ⌘←/→ = Home/End, ⌥←/→ = palavra, ⌘V/⌘C pela área de transferência real.
const IS_MAC = window.__FORCE_MAC ?? /Mac/i.test(navigator.platform);
const XK = { ctrl: [0xffe3, "ControlLeft"], shift: [0xffe1, "ShiftLeft"], home: [0xff50, "Home"], end: [0xff57, "End"],
  left: [0xff51, "ArrowLeft"], right: [0xff53, "ArrowRight"], up: [0xff52, "ArrowUp"], down: [0xff54, "ArrowDown"], bs: [0xff08, "Backspace"], del: [0xffff, "Delete"] };
const swallowed = new Set();
const press = (k) => [[...XK[k], true], [...XK[k], false]];
const chord = (mods, key) => [...mods.map((m) => [...XK[m], true]), ...(Array.isArray(key[0]) ? key : [[...key, true], [...key, false]]), ...[...mods].reverse().map((m) => [...XK[m], false])];
function vmSend(seq) { const r = S.screen?.rfb; if (r) for (const [ks, code, down] of seq) r.sendKey(ks, code, down); }
const stop = (e) => { e.preventDefault(); e.stopImmediatePropagation(); };
async function pullClipboard() {
  const sc = S.screen; if (!sc) return;
  try { const n = await invoke("clipboard_from_vm", { name: sc.name }); if (n > 0) toast("Copiado da máquina"); } catch {}
}
async function pasteToVm(mods) {
  const sc = S.screen; if (!sc) return;
  try { await invoke("clipboard_to_vm", { name: sc.name }); } catch (e) { return toast(esc(e), { err: true }); }
  vmSend(chord(mods, [0x76, "KeyV"]));
}
function vmKeydown(e) {
  if (!(S.control && S.view === "focus" && S.screen?.rfb) || S.k || $("#ctx").innerHTML) return false;
  const k = e.key, cmd = IS_MAC ? e.metaKey : e.ctrlKey;
  // atalhos que continuam sendo do app mesmo com o controle
  if (IS_MAC && e.metaKey && (k.toLowerCase() === "k" || k === "Escape" || (e.shiftKey && k.toLowerCase() === "s"))) return false;
  if (k === "CapsLock" || k === "Meta" && IS_MAC) { swallowed.add(e.code); stop(e); return true; } // Caps: o case já vem em e.key; encaminhar inverte duas vezes
  const mods = e.shiftKey ? ["ctrl", "shift"] : ["ctrl"];
  const lk = k.length === 1 ? k.toLowerCase() : "";
  const done = (seq, after) => { swallowed.add(e.code); stop(e); if (!e.repeat) { if (seq) vmSend(seq); after?.(); } return true; };
  if (cmd && lk === "v") { swallowed.add(e.code); stop(e); if (!e.repeat) pasteToVm(mods); return true; }
  if (IS_MAC && e.metaKey) {
    if (lk === "c" || lk === "x") return done(chord(mods, [lk.charCodeAt(0), e.code]), () => setTimeout(pullClipboard, 300));
    const nav = { ArrowLeft: chord([], XK.home), ArrowRight: chord([], XK.end), ArrowUp: chord(["ctrl"], XK.home), ArrowDown: chord(["ctrl"], XK.end),
      Backspace: [...chord(["shift"], XK.home), ...press("del")] }[k];
    if (nav) return done(e.shiftKey && (k === "ArrowLeft" || k === "ArrowRight") ? chord(["shift"], k === "ArrowLeft" ? XK.home : XK.end) : nav);
    if (lk) return done(chord(mods, [lk.charCodeAt(0), e.code]));
    return done(null);
  }
  if (!IS_MAC && cmd && (lk === "c" || lk === "x")) setTimeout(pullClipboard, 300);       // passa direto; só sincroniza depois
  if (IS_MAC && e.altKey && !e.metaKey) {
    const w = { ArrowLeft: chord(["ctrl"], XK.left), ArrowRight: chord(["ctrl"], XK.right), Backspace: chord(["ctrl"], XK.bs) }[k];
    if (w) return done(w);
  }
  return false;
}
function vmKeyup(e) { if (swallowed.has(e.code)) { swallowed.delete(e.code); stop(e); return true; } return false; }
addEventListener("keydown", (e) => { vmKeydown(e); }, true);
addEventListener("keyup", (e) => { vmKeyup(e); }, true);
addEventListener("focus", () => { if (S.control) S.screen?.rfb?.focus?.(); });

// Campos do próprio app (⌘K, console): sem menu Editar, ⌘V/⌘C/⌘X/⌘A/⌘Z são tratados aqui, pela área de transferência real.
addEventListener("keydown", async (e) => {
  const cmd = IS_MAC ? e.metaKey : false, t = e.target;
  if (!cmd || !t?.matches?.("input, textarea") || e.altKey) return;
  const k = e.key.toLowerCase();
  const sel = () => t.value.slice(t.selectionStart, t.selectionEnd);
  if (k === "v") { stop(e); try { document.execCommand("insertText", false, await invoke("clipboard_read")); t.dispatchEvent(new Event("input", { bubbles: true })); } catch {} }
  else if (k === "c") { stop(e); if (sel()) invoke("clipboard_write", { text: sel() }).catch(() => {}); }
  else if (k === "x") { stop(e); if (sel()) { await invoke("clipboard_write", { text: sel() }).catch(() => {}); document.execCommand("delete"); } }
  else if (k === "a") { stop(e); t.select(); }
  else if (k === "z") { stop(e); document.execCommand(e.shiftKey ? "redo" : "undo"); }
}, true);

/* ================================================================== laços leves */
// Miniaturas: só desktops ligados, só com a grade visível e o app em primeiro plano.
(async function thumbLoop() {
  for (;;) {
    if (!document.hidden && (S.view === "grid" || S.k)) {
      for (const m of S.machines.filter((x) => x.has_screen && x.state === "running")) {
        try { S.thumbs[m.name] = await invoke("machine_thumb", { name: m.name }); if (S.view === "grid") paintTile(m.name); } catch {}
      }
    }
    await sleep(4000);
  }
})();
// Uso: 3 s para a máquina em foco com ⌘I aberto; 15 s para as demais (rodapé dos cartões).
(async function statsLoop() {
  let tick = 0;
  for (;;) {
    if (!document.hidden) {
      const focus = S.view === "focus" && S.info ? cur() : null;
      const running = S.machines.filter((m) => m.state === "running");
      const list = tick % 5 === 0 || running.some((m) => !S.stats[m.name]) ? running : focus && focus.state === "running" ? [focus] : [];
      for (const m of list) {
        try {
          const st = await invoke("machine_stats", { name: m.name });
          const prev = S.stats[m.name];
          if (prev && st.cpu_total > prev.cpu_total) S.cpu[m.name] = (1 - (st.cpu_idle - prev.cpu_idle) / (st.cpu_total - prev.cpu_total)) * 100;
          S.stats[m.name] = st;
          paintInfoStats(m);
          if (S.view === "grid") paintTile(m.name);
        } catch {}
      }
    }
    tick++; await sleep(3000);
  }
})();

refresh(true);
setInterval(refresh, 2000);
setTimeout(() => checkUpdate(false), 4000);          // sem atrasar a abertura
setInterval(() => checkUpdate(false), 6 * 3600 * 1000);
