// Painel da bandeja (barra de menus do Mac / área de notificação do Windows): máquinas, perguntas do agente e ações rápidas.
const invoke = window.__TAURI__?.core?.invoke ?? (() => Promise.reject("sem Tauri"));
const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
let ms = [], sig = "";
const label = (m) => (m.takeover ? m.takeover : { running: "ligada", paused: "pausada", stopped: "desligada" }[m.state] || m.state) + (m.always_on ? " · 24 h" : "");

function render() {
  const need = ms.filter((m) => m.takeover).length + ms.reduce((n, m) => n + (m.asks?.length || 0), 0);
  const asks = ms.flatMap((m) => (m.asks || []).map((a) => ({ ...a, name: m.name })));
  $("#tray").innerHTML = `<header><b>HyzeVM</b><span>${ms.filter((m) => m.state === "running").length} ligada(s)${need ? ` · <b style="color:#e5a50a">${need} precisa(m) de você</b>` : ""}</span></header>
    <div class="list">
      ${asks.length ? `<div class="sect">Perguntas</div>` + asks.slice(0, 3).map((a) => `<div class="q" data-n="${esc(a.name)}" data-id="${esc(a.id)}"><small>${esc(a.name)}</small><p>${esc(a.question)}</p>
        ${a.options.length ? `<div class="opts">${a.options.map((o) => `<button class="pill soft" data-ans="${esc(o)}">${esc(o)}</button>`).join("")}</div>` : `<form><input placeholder="Sua resposta…"><button class="pill solid">Enviar</button></form>`}</div>`).join("") : ""}
      ${ms.some((m) => m.takeover) ? `<div class="sect">Precisam de você</div>` + ms.filter((m) => m.takeover).map((m) => `<div class="m" data-open="${esc(m.name)}"><span class="dot wait"></span><div class="n"><b>${esc(m.name)}</b><small>${esc(m.takeover)}</small></div><button title="Assumir" data-open="${esc(m.name)}">↗</button></div>`).join("") : ""}
      <div class="sect">Máquinas</div>
      ${ms.length ? ms.map((m) => `<div class="m" data-open="${esc(m.name)}"><span class="dot ${m.takeover ? "wait" : m.state === "running" ? "running" : ""}"></span><div class="n"><b>${esc(m.name)}</b><small>${esc(label(m))}</small></div>
        <button title="${m.state === "running" ? "Pausar" : "Ligar"}" data-pp="${esc(m.name)}" data-st="${m.state}">${m.state === "running" ? "❚❚" : "▶"}</button></div>`).join("") : `<p style="padding:8px 14px;color:var(--fg-3,#888)">Nenhuma máquina ainda.</p>`}
    </div>
    <footer><button class="pill soft" data-f="pauseall">Pausar tudo</button><button class="pill solid" data-f="open">Abrir HyzeVM</button><button class="pill soft" data-f="quit" title="Sair do HyzeVM">Sair</button></footer>`;
}

async function refresh() {
  try {
    ms = await invoke("list_machines");
    const s = JSON.stringify(ms);
    if (s !== sig && !document.activeElement?.closest?.("form")) { sig = s; render(); }
    invoke("tray_badge", { count: ms.filter((m) => m.takeover).length + ms.reduce((n, m) => n + (m.asks?.length || 0), 0) }).catch(() => {});
  } catch {}
}

document.addEventListener("click", async (e) => {
  const t = e.target;
  try {
    const ans = t.closest("[data-ans]");
    if (ans) { const q = ans.closest(".q"); await invoke("ask_answer", { name: q.dataset.n, id: q.dataset.id, text: ans.dataset.ans }); return refresh(); }
    const pp = t.closest("[data-pp]");
    if (pp) { await invoke("machine_action", { name: pp.dataset.pp, action: pp.dataset.st === "running" ? "suspend" : "start" }); return refresh(); }
    const f = t.closest("[data-f]");
    if (f?.dataset.f === "open") return invoke("show_main", { name: null });
    if (f?.dataset.f === "quit") return invoke("quit_app");
    if (f?.dataset.f === "pauseall") { await invoke("pause_all"); return refresh(); }
    const o = t.closest("[data-open]");
    if (o) return invoke("show_main", { name: o.dataset.open });
  } catch {}
});
document.addEventListener("submit", async (e) => {
  e.preventDefault();
  const q = e.target.closest(".q"), v = e.target.querySelector("input").value.trim();
  if (q && v) { try { await invoke("ask_answer", { name: q.dataset.n, id: q.dataset.id, text: v }); } catch {} sig = ""; refresh(); }
});
refresh(); setInterval(refresh, 2500);
addEventListener("focus", refresh);
