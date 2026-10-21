#!/bin/bash
# Teste de ponta a ponta do HyzeVM: cria uma máquina mínima e exercita o essencial.
# Uso: HZ=target/release/hyzevm tests/e2e.sh   (precisa de QEMU; leva ~1 min com a imagem em cache)
set -u
HZ="${HZ:-hyzevm}"
N="e2e-$$"
pass=0; fail=0
ok()  { pass=$((pass+1)); echo "  ✓ $1"; }
bad() { fail=$((fail+1)); echo "  ✗ $1"; }
check() { local d="$1"; shift; if "$@" >/dev/null 2>&1; then ok "$d"; else bad "$d"; fi; }
eq() { local d="$1" want="$2"; shift 2; local got; got="$("$@" 2>/dev/null)"; if [ "$got" = "$want" ]; then ok "$d"; else bad "$d (esperava '$want', veio '$got')"; fi; }
cleanup() { "$HZ" rm "$N" >/dev/null 2>&1; "$HZ" snapshot rm "$N-s" >/dev/null 2>&1; "$HZ" rm "$N-c" >/dev/null 2>&1; rm -f "/tmp/$N.txt"; }
trap cleanup EXIT

echo "criar"
check "create minimal" "$HZ" create "$N"
eq "exec" "oi" "$HZ" exec "$N" -- echo oi

echo "arquivos"
echo "conteudo" > "/tmp/$N.txt"
check "cp para a máquina" "$HZ" cp "/tmp/$N.txt" "$N:/tmp/e2e.txt"
eq "cat do arquivo copiado" "conteudo" "$HZ" exec "$N" -- cat /tmp/e2e.txt

echo "pausar e retomar"
check "pause" "$HZ" pause "$N"
eq "estado pausada" "paused" bash -c "$HZ get $N --json | python3 -c 'import sys,json;print(json.load(sys.stdin)[\"state\"])'"
eq "exec retoma sozinha" "volta" "$HZ" exec "$N" -- echo volta

echo "ponto de restauração"
"$HZ" exec "$N" -- 'echo antes > /home/agent/f.txt; sync' >/dev/null 2>&1
check "checkpoint save" "$HZ" checkpoint save "$N" p1
"$HZ" exec "$N" -- 'echo depois > /home/agent/f.txt; sync' >/dev/null 2>&1
check "checkpoint restore" "$HZ" checkpoint restore "$N" p1
eq "arquivo voltou ao ponto" "antes" "$HZ" exec "$N" -- cat /home/agent/f.txt

echo "registro de ações"
check "activity tem exec" bash -c "$HZ activity $N --json | grep -q '\"kind\":\"exec\"'"

echo "pergunta ao humano"
ID="$("$HZ" ask send "$N" "ok?" --options sim,nao 2>/dev/null)"
check "ask ls mostra" bash -c "$HZ ask ls $N | grep -q '$ID'"
check "resposta inválida é recusada" bash -c "! $HZ ask answer $N $ID talvez"
"$HZ" ask answer "$N" "$ID" sim >/dev/null 2>&1
eq "ask wait devolve a resposta" "sim" "$HZ" ask wait "$N" "$ID" --timeout 5

echo "trava de ações"
check "guard set" "$HZ" guard set "$N" --require delete
( sleep 3; I="$("$HZ" ask ls "$N" | tail -1 | cut -f2)"; "$HZ" ask answer "$N" "$I" não >/dev/null 2>&1 ) &
bad_exit=0; "$HZ" exec "$N" -- rm -rf /tmp/nada >/dev/null 2>&1 || bad_exit=1
[ "$bad_exit" = 1 ] && ok "comando destrutivo recusado" || bad "comando destrutivo deveria ser barrado"
wait
check "guard off" "$HZ" guard off "$N"

echo "clonar"
"$HZ" stop "$N" >/dev/null 2>&1
check "clone" "$HZ" clone "$N" "$N-c"
eq "clone tem o arquivo" "antes" "$HZ" exec "$N-c" -- cat /home/agent/f.txt

echo "desligar"
check "stop" "$HZ" stop "$N-c"

echo; echo "resultado: $pass ok, $fail falhas"
[ "$fail" = 0 ]
