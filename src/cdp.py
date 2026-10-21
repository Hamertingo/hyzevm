#!/usr/bin/env python3
"""hyzevm cdp — fala com o Chromium do desktop via DevTools Protocol (só stdlib, só loopback)."""
import base64, hashlib, json, os, socket, struct, sys, time, urllib.request

import os as _os
PORT = int(_os.environ.get('HYZE_CDP_PORT', '9222'))

def http(path, method="GET"):
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}{path}", method=method)
    return json.load(urllib.request.urlopen(req, timeout=5))

class WS:
    def __init__(self, url):
        host, rest = url[5:].split("/", 1)
        h, p = host.split(":")
        self.s = socket.create_connection((h, int(p)), timeout=30)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall((f"GET /{rest} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                        f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self.s.recv(4096)
        self.buf = buf.split(b"\r\n\r\n", 1)[1]
        self.n = 0

    def _read(self, n):
        while len(self.buf) < n:
            d = self.s.recv(65536)
            if not d: raise EOFError("conexão CDP fechada")
            self.buf += d
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def send(self, obj):
        data = json.dumps(obj).encode()
        mask = os.urandom(4)
        n = len(data)
        hdr = b"\x81" + (bytes([0x80 | n]) if n < 126 else b"\xfe" + struct.pack(">H", n) if n < 65536 else b"\xff" + struct.pack(">Q", n))
        self.s.sendall(hdr + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def recv(self):
        msg = b""
        while True:
            b0, b1 = self._read(2)
            n = b1 & 0x7F
            if n == 126: n = struct.unpack(">H", self._read(2))[0]
            elif n == 127: n = struct.unpack(">Q", self._read(8))[0]
            data = self._read(n)
            op = b0 & 0x0F
            if op == 8: raise EOFError("CDP fechou")
            if op in (9, 10): continue
            msg += data
            if b0 & 0x80: return json.loads(msg)

    def call(self, method, params=None):
        self.n += 1
        i = self.n
        self.send({"id": i, "method": method, "params": params or {}})
        while True:
            m = self.recv()
            if m.get("id") == i:
                if "error" in m: raise RuntimeError(m["error"].get("message", str(m["error"])))
                return m.get("result", {})

def page():
    pages = [t for t in http("/json") if t.get("type") == "page"]
    if not pages:
        pages = [http("/json/new?about:blank", "PUT")]
    return WS(pages[0]["webSocketDebuggerUrl"])

def ev(ws, js, by_value=True):
    r = ws.call("Runtime.evaluate", {"expression": js, "returnByValue": by_value, "awaitPromise": True})
    if "exceptionDetails" in r:
        raise RuntimeError(r["exceptionDetails"].get("exception", {}).get("description", "erro no JS"))
    return r["result"].get("value") if by_value else r["result"]

SNAP = r"""(()=>{document.querySelectorAll('[data-hyze]').forEach(e=>e.removeAttribute('data-hyze'));
const sel='a[href],button,input,textarea,select,summary,[role=button],[role=link],[role=textbox],[role=menuitem],[role=tab],[role=checkbox],[role=option],[contenteditable=""],[contenteditable=true],[onclick]';
let i=0;const out=[];
for(const e of document.querySelectorAll(sel)){
 const r=e.getBoundingClientRect();const cs=getComputedStyle(e);
 if(r.width<2||r.height<2||cs.visibility==='hidden'||cs.display==='none')continue;
 if(r.bottom<0||r.top>innerHeight*4)continue;
 i++;e.setAttribute('data-hyze',i);
 const t=(e.type||'');
 const label=(e.getAttribute('aria-label')||e.innerText||((t==='button'||t==='submit')?e.value:'')||e.placeholder||e.title||e.alt||'').trim().replace(/\s+/g,' ').slice(0,80);
 out.push({ref:'e'+i,tag:e.getAttribute('role')||e.tagName.toLowerCase(),type:t,label,href:e.tagName==='A'?e.href.slice(0,100):'',disabled:!!e.disabled,checked:e.checked===true,inview:r.top>=0&&r.bottom<=innerHeight});
 if(i>=250)break;}
return {url:location.href,title:document.title,els:out,scrollY:Math.round(scrollY),height:document.documentElement.scrollHeight,vh:innerHeight,
 text:(document.body?document.body.innerText:'').replace(/\n{3,}/g,'\n\n').slice(0,2500)};})()"""

def wait_load(ws, secs=20):
    t = time.time()
    while time.time() - t < secs:
        try:
            if ev(ws, "document.readyState") == "complete": return
        except Exception: pass
        time.sleep(0.3)

def center(ws, ref):
    n = ref[1:] if ref.startswith("e") else ref
    r = ev(ws, f"""(()=>{{const e=document.querySelector('[data-hyze="{n}"]');if(!e)return null;
e.scrollIntoView({{block:'center',inline:'center'}});const r=e.getBoundingClientRect();return {{x:r.left+r.width/2,y:r.top+r.height/2}};}})()""")
    if not r: raise RuntimeError(f"referência {ref} não existe mais — rode `browser snapshot` de novo")
    return r

KEYS = {"enter": ("Enter", "Enter", 13, "\r"), "tab": ("Tab", "Tab", 9, ""), "escape": ("Escape", "Escape", 27, ""), "esc": ("Escape", "Escape", 27, ""),
        "backspace": ("Backspace", "Backspace", 8, ""), "delete": ("Delete", "Delete", 46, ""), "space": (" ", "Space", 32, " "),
        "arrowdown": ("ArrowDown", "ArrowDown", 40, ""), "arrowup": ("ArrowUp", "ArrowUp", 38, ""), "arrowleft": ("ArrowLeft", "ArrowLeft", 37, ""),
        "arrowright": ("ArrowRight", "ArrowRight", 39, ""), "pagedown": ("PageDown", "PageDown", 34, ""), "pageup": ("PageUp", "PageUp", 33, ""),
        "home": ("Home", "Home", 36, ""), "end": ("End", "End", 35, "")}

def press(ws, spec):
    parts = spec.lower().split("+")
    mods = 0
    for m in parts[:-1]: mods |= {"alt": 1, "ctrl": 2, "control": 2, "meta": 4, "cmd": 4, "shift": 8}.get(m, 0)
    k = parts[-1]
    key, code, vk, text = KEYS.get(k, (k, "Key" + k.upper() if len(k) == 1 else k, ord(k.upper()) if len(k) == 1 else 0, k if len(k) == 1 else ""))
    base = {"modifiers": mods, "key": key, "code": code, "windowsVirtualKeyCode": vk}
    ws.call("Input.dispatchKeyEvent", {**base, "type": "keyDown", **({"text": text} if text and not mods & 6 else {})})
    ws.call("Input.dispatchKeyEvent", {**base, "type": "keyUp"})

def main():
    a = sys.argv[1:]
    cmd = a[0] if a else ""
    if cmd == "tabs":
        for i, t in enumerate(x for x in http("/json") if x.get("type") == "page"):
            print(f"{i}\t{t['title'][:50]}\t{t['url']}")
        return
    ws = page()
    ws.call("Page.enable")
    if cmd == "goto":
        ws.call("Page.navigate", {"url": a[1]}); time.sleep(0.4); wait_load(ws)
        print(json.dumps({"url": ev(ws, "location.href"), "title": ev(ws, "document.title")}, ensure_ascii=False))
    elif cmd == "snapshot":
        d = ev(ws, SNAP)
        print(f"url: {d['url']}\ntitle: {d['title']}\nscroll: {d['scrollY']}/{d['height']} (viewport {d['vh']})")
        print("--- elementos interativos")
        for e in d["els"]:
            extra = " ".join(x for x in [f"type={e['type']}" if e["type"] and e["tag"] in ("input", "button") else "", "desabilitado" if e["disabled"] else "",
                                          "marcado" if e["checked"] else "", f"-> {e['href']}" if e["href"] else "", "" if e["inview"] else "(fora da tela)"] if x)
            print(f"[{e['ref']}] {e['tag']} \"{e['label']}\" {extra}".rstrip())
        print("--- texto visível\n" + d["text"])
    elif cmd == "click":
        c = center(ws, a[1]); time.sleep(0.15)
        for t in ("mouseMoved", "mousePressed", "mouseReleased"):
            ws.call("Input.dispatchMouseEvent", {"type": t, "x": c["x"], "y": c["y"], "button": "left", "clickCount": 1, "buttons": 1 if t == "mousePressed" else 0})
        time.sleep(0.5); print("ok")
    elif cmd == "type":
        ref, text, submit = a[1], a[2], "--submit" in a[3:]
        center(ws, ref)
        n = ref[1:]
        ev(ws, f"""(()=>{{const e=document.querySelector('[data-hyze="{n}"]');e.focus();
if('value' in e){{e.select&&e.select();}}else{{document.execCommand('selectAll');}}}})()""")
        ws.call("Input.insertText", {"text": text})
        if submit: press(ws, "enter")
        print("ok")
    elif cmd == "press":
        press(ws, a[1]); time.sleep(0.2); print("ok")
    elif cmd == "scroll":
        d, amt = a[1], int(a[2]) if len(a) > 2 else 5
        dx, dy = {"down": (0, 1), "up": (0, -1), "right": (1, 0), "left": (-1, 0)}[d]
        ws.call("Input.dispatchMouseEvent", {"type": "mouseWheel", "x": 400, "y": 400, "deltaX": dx * 100 * amt, "deltaY": dy * 100 * amt})
        time.sleep(0.4); print("ok")
    elif cmd == "upload":
        n = a[1][1:]
        obj = ev(ws, f"document.querySelector('[data-hyze=\"{n}\"]')", by_value=False)
        if not obj.get("objectId"): raise RuntimeError(f"referência {a[1]} não existe mais")
        ws.call("DOM.setFileInputFiles", {"objectId": obj["objectId"], "files": a[2:]})
        time.sleep(0.5); print("ok")
    elif cmd == "wait":
        arg = a[1]
        if arg.isdigit(): time.sleep(int(arg) / 1000)
        else:
            t = time.time()
            while time.time() - t < (int(a[2]) if len(a) > 2 else 20):
                if arg in ev(ws, "document.body?document.body.innerText:''"): print("ok"); return
                time.sleep(0.5)
            raise RuntimeError(f"texto “{arg}” não apareceu")
        print("ok")
    else:
        raise RuntimeError("comandos: tabs goto snapshot click type press scroll upload wait")

try:
    main()
except Exception as e:
    print(f"erro: {e}", file=sys.stderr)
    sys.exit(1)
