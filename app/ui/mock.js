// Dados de teste: só entra quando a página abre fora do app (preview no navegador).
if (!window.__TAURI__) {
  window.__MOCK_SCREEN__ = true;
  window.__FORCE_MAC = true;
  document.addEventListener("DOMContentLoaded", () => document.body.classList.add("web"));
  let machines = [
    { name: "agente-videos", state: "running", template: "desktop", cpus: 4, memory_mb: 4096, disk_gb: 32, always_on: true, has_screen: true, ports: [], takeover: "CAPTCHA no login do Instagram", asks: [{ id: "a1", question: "Posto o vídeo \"Tutorial 03\" agora no Instagram?", options: ["sim", "não"] }, { id: "a2", question: "Qual legenda uso?", options: [] }], auto_suspend_secs: 0, ssh_port: 1, meta: {} },
    { name: "agente-pesquisa", state: "running", template: "desktop", cpus: 4, memory_mb: 4096, disk_gb: 32, always_on: false, has_screen: true, ports: [], takeover: null, asks: [], auto_suspend_secs: 1800, ssh_port: 2, meta: {} },
    { name: "build-ci", state: "running", template: "base", cpus: 2, memory_mb: 2048, disk_gb: 32, always_on: false, has_screen: false, ports: [], takeover: null, asks: [], auto_suspend_secs: 1800, ssh_port: 3, meta: {} },
    { name: "edicao", state: "paused", template: "desktop", cpus: 4, memory_mb: 4096, disk_gb: 32, always_on: false, has_screen: true, ports: [], takeover: null, asks: [], auto_suspend_secs: 1800, ssh_port: 4, meta: {} },
  ];
  const svg = (title, accent) => "data:image/svg+xml;utf8," + encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1280 800">
    <rect width="1280" height="800" fill="#1b1b1d"/><rect width="1280" height="704" fill="#fff"/><rect width="1280" height="40" fill="#e3e7ee"/>
    <rect x="10" y="8" width="250" height="32" rx="10" fill="#fff"/><rect x="24" y="16" width="16" height="16" rx="4" fill="${accent}"/>
    <text x="50" y="29" font-family="Arial" font-size="13" fill="#222">${title}</text>
    <rect x="80" y="47" width="1180" height="32" rx="16" fill="#eef0f3"/><rect x="0" y="86" width="1280" height="56" fill="#fff"/>
    <rect x="210" y="170" width="860" height="470" rx="12" fill="#fff" stroke="#e5e5e5"/><text x="236" y="212" font-family="Arial" font-size="20" font-weight="600" fill="#111">${title}</text>
    <rect x="236" y="250" width="500" height="44" rx="6" fill="none" stroke="#ccc"/><rect x="236" y="310" width="500" height="120" rx="6" fill="none" stroke="#ccc"/>
    <rect x="760" y="250" width="285" height="170" rx="8" fill="#3a3f47"/><rect x="236" y="590" width="640" height="6" rx="3" fill="#e5e5e5"/><rect x="236" y="590" width="410" height="6" rx="3" fill="#065fd4"/>
    <rect x="930" y="575" width="110" height="36" rx="18" fill="#065fd4"/>
    <rect x="534" y="716" width="212" height="76" rx="22" fill="#ffffff" fill-opacity=".14" stroke="#fff" stroke-opacity=".22"/>
    <circle cx="576" cy="754" r="26" fill="#4285f4"/><rect x="618" y="728" width="52" height="52" rx="13" fill="#26262a"/><rect x="686" y="728" width="52" height="52" rx="13" fill="#ddd"/></svg>`);
  const thumbs = { "agente-videos": svg("Enviar vídeos — YouTube Studio", "#ff0033"), "agente-pesquisa": svg("tendências edição de vídeo", "#4285f4"), edicao: svg("Edição — rascunho", "#888") };
  const fakeDesktop = (name) => URL.createObjectURL(new Blob([`<body style="margin:0;background:#000"><img src="${thumbs[name] || thumbs["agente-pesquisa"]}" style="width:100vw;height:100vh;object-fit:cover"></body>`], { type: "text/html" }));
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const st = { t: 0 };
  const listeners = [];
  const H = {
    platform: async () => "macos",
    list_machines: async () => JSON.parse(JSON.stringify(machines)),
    machine_action: async ({ name, action }) => {
      await wait(600);
      const m = machines.find((x) => x.name === name);
      if (action === "remove") machines = machines.filter((x) => x !== m);
      else if (action === "start") m.state = "running"; else if (action === "suspend") m.state = "paused"; else m.state = "stopped";
    },
    create_machine: async ({ req }) => {
      for (const s of ["criando disco", "ligando", "aguardando o boot", "  8s  Started ssh.service"]) { listeners.forEach((f) => f({ payload: { name: req.name, msg: s } })); await wait(800); }
      machines.push({ name: req.name, state: "running", template: req.template, cpus: req.cpus, memory_mb: req.memory_gb * 1024, disk_gb: req.disk_gb, always_on: req.always_on, has_screen: req.template === "desktop", ports: [], takeover: null, asks: [], auto_suspend_secs: 1800, ssh_port: 9, meta: {} });
      thumbs[req.name] = thumbs["agente-pesquisa"];
    },
    machine_thumb: async ({ name }) => { if (!thumbs[name]) throw "sem tela"; return thumbs[name]; },
    screen_ws: async ({ name }) => fakeDesktop(name),
    machine_stats: async () => { st.t += 3; return { cpu_total: st.t * 400, cpu_idle: st.t * 400 - st.t * 150, cores: 4, mem_total_mb: 3924, mem_used_mb: 1630, disk_total_gb: 31.2, disk_used_gb: 4.6, uptime_secs: 11540 + st.t }; },
    set_auto_suspend: async ({ name, secs }) => { const m = machines.find((x) => x.name === name); m.auto_suspend_secs = secs; },
    set_always_on: async ({ name, on }) => { const m = machines.find((x) => x.name === name); m.always_on = on; },
    ask_answer: async ({ name, id }) => { const m = machines.find((x) => x.name === name); m.asks = m.asks.filter((a) => a.id !== id); },
    takeover_done: async ({ name }) => { machines.find((x) => x.name === name).takeover = null; },
    exec: async ({ command }) => ({ code: 0, stdout: command.startsWith("uname") ? "Linux build-ci 6.1.0-44-arm64 aarch64 GNU/Linux\n" : "ok\n", stderr: "" }),
    clipboard_read: async () => "texto do mac", clipboard_write: async () => {}, clipboard_to_vm: async () => 11, clipboard_from_vm: async () => 5,
    update_check: async () => ({ version: "0.2.0", current: "0.1.0", notes: "Atualização automática dentro do app\nFuso e idioma das máquinas iguais aos do seu computador\nCorreções no menu de botão direito", page: "https://github.com/Hamertingo/hyzevm/releases", can_install: true }),
    update_install: async () => { await wait(2500); },
    open_url: async () => {},
    desktop_type: async () => {}, save_screenshot: async () => "/tmp/x.png", open_terminal: async () => {},
  };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => { if (!H[cmd]) throw "mock sem " + cmd; return H[cmd](args || {}); } },
    event: { listen: async (_n, f) => { listeners.push(f); return () => {}; } },
  };
}
