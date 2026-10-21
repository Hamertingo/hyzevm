// Visor de tela do app: cliente RFB (noVNC core) embutido, sem a interface do noVNC.
// Fora do app (preview no navegador com mock) mostra um desktop de mentira.
import RFB from "./vendor/novnc/core/rfb.js";

export function openScreen(host, url, on = {}) {
  if (window.__MOCK_SCREEN__) return mockScreen(host, url, on);
  const rfb = new RFB(host, url, { shared: true });
  rfb.scaleViewport = true;      // encaixa na moldura mantendo a proporção
  rfb.resizeSession = false;     // o desktop tem tamanho fixo (1280×800)
  rfb.background = "#000";
  rfb.qualityLevel = 8;
  rfb.compressionLevel = 2;
  rfb.addEventListener("connect", () => on.connect?.());
  rfb.addEventListener("disconnect", (e) => on.disconnect?.(e.detail?.clean));
  return rfb;
}

function mockScreen(host, url, on) {
  const f = document.createElement("iframe");
  f.style.cssText = "width:100%;height:100%;border:0;background:#000";
  f.src = url; host.appendChild(f);
  setTimeout(() => on.connect?.(), 350);
  return {
    disconnect() { f.remove(); },
    focus() {}, viewOnly: true,
    sendKey(ks, code, down) { (window.__sent ||= []).push([ks, code, down]); },
    toDataURL: () => "data:image/png;base64,iVBORw0KGgo=",
    get size() { return [1280, 800]; },
  };
}

export function screenSize(host) {
  const c = host.querySelector("canvas");
  return c && c.width ? [c.width, c.height] : null;
}
