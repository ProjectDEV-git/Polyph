#!/usr/bin/env bash
# Checks a release archive the way someone installing it would use it:
# the binaries run, the relay answers /healthz, and the daemon serves the
# browser client from share/polyph/web next to its binary.
#
# Usage: scripts/release/smoke.sh dist/polyph-<version>-<target>.tar.gz|.zip
set -euo pipefail

archive=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$(mktemp -d)
pids=()
cleanup() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  if [ -n "${HOME_DIR:-}" ] && [ -x "${BIN:-}/polyph$EXE" ]; then
    "$BIN/polyph$EXE" --home "$HOME_DIR" daemon stop >/dev/null 2>&1 || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT

cd "$work"
case "$archive" in
  *.zip)
    if command -v unzip >/dev/null 2>&1; then unzip -q "$archive"
    else powershell -NoProfile -Command "Expand-Archive -LiteralPath '$(cygpath -w "$archive" 2>/dev/null || echo "$archive")' -DestinationPath ."
    fi ;;
  *) tar -xzf "$archive" ;;
esac
dir=$(find . -maxdepth 1 -mindepth 1 -type d -name 'polyph-*' | head -n1)
BIN="$work/$dir/bin"
EXE=""
[ -f "$BIN/polyph.exe" ] && EXE=".exe"

echo "== versions"
"$BIN/polyph$EXE" --version
"$BIN/polyph-relay$EXE" --version

echo "== relay"
port=$(( 20000 + RANDOM % 20000 ))
POLYPH_RELAY_BIND="127.0.0.1:$port" POLYPH_RELAY_DATABASE=":memory:" "$BIN/polyph-relay$EXE" >relay.log 2>&1 &
pids+=($!)
for _ in $(seq 1 50); do
  curl -fsS "http://127.0.0.1:$port/healthz" >/dev/null 2>&1 && break
  sleep 0.2
done
curl -fsS "http://127.0.0.1:$port/healthz"
echo

echo "== daemon and browser client"
HOME_DIR="$work/home"
export POLYPH_KEYSTORE=file
# Written before anything starts the daemon, so it picks a free port.
mkdir -p "$HOME_DIR"
printf 'api_port = 0\nrelay = "ws://127.0.0.1:%s/ws"\n' "$port" >"$HOME_DIR/config.toml"
link=$("$BIN/polyph$EXE" --home "$HOME_DIR" web --no-open | grep -Eo 'http://[^ ]+' | head -n1)
base=${link%%#*}
code=$(curl -s -o /dev/null -w '%{http_code}' "$base")
[ "$code" = "200" ] || { echo "browser client answered $code at $base"; exit 1; }
echo "browser client served at $base"

echo "smoke test passed: $(basename "$archive")"
