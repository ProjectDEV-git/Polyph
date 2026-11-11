#!/usr/bin/env bash
# Multi-network test (KAO-72): three daemons in three network namespaces that
# cannot reach each other, talking only through a relay, run the PRODUCT.md
# success path (crates/polyph-testkit/tests/multinet.rs).
#
#   scripts/multinet.sh                 # local polyph-relay inside the hub namespace
#   POLYPH_MULTINET_RELAY=wss://polyph-relay.fly.dev/ws scripts/multinet.sh
#                                       # the real relay, reached through NAT on the host
#
# Topology: pm-a, pm-b, pm-c each have one veth to pm-hub (10.77.N.0/24, hub is
# .1). The hub does not forward between them, so the only thing the three can
# reach is the relay. With a real relay the hub forwards to the internet, with
# a firewall rule that still drops node-to-node traffic.
#
# Needs root for `ip netns` and passwordless sudo (GitHub ubuntu runners have
# it). There is no rootless mode: `unshare -rn` gives a namespace with no link
# to another one without slirp4netns or pasta, and a link per daemon to a hub
# is what this setup needs.
set -euo pipefail

cd "$(dirname "$0")/.."
root() { if [ "$(id -u)" = 0 ]; then "$@"; else sudo -n "$@"; fi; }
root true || { echo "multinet: needs root or passwordless sudo" >&2; exit 2; }

NODES=(pm-a pm-b pm-c)
HUB=pm-hub
UPLINK_NET=10.77.9
PORT=8787
EXT_RELAY="${POLYPH_MULTINET_RELAY:-}"
RELAY_PID=""

cleanup() {
  [ -n "$RELAY_PID" ] && root kill "$RELAY_PID" 2>/dev/null || true
  for ns in "${NODES[@]}" "$HUB"; do
    root ip netns del "$ns" 2>/dev/null || true
    root rm -rf "/etc/netns/$ns"
  done
  root ip link del pm-up 2>/dev/null || true
  if [ -n "$EXT_RELAY" ]; then
    root iptables -D FORWARD -s 10.77.0.0/16 -j ACCEPT 2>/dev/null || true
    root iptables -D FORWARD -d 10.77.0.0/16 -j ACCEPT 2>/dev/null || true
    root iptables -t nat -D POSTROUTING -s 10.77.0.0/16 ! -d 10.77.0.0/16 -j MASQUERADE 2>/dev/null || true
  fi
}
trap cleanup EXIT
cleanup

nsx() { local ns=$1; shift; root ip netns exec "$ns" "$@"; }

cargo build --locked -p polyph-testkit --bins -p polyph-relay
target="${CARGO_TARGET_DIR:-target}"

root ip netns add "$HUB"
nsx "$HUB" ip link set lo up
for i in 0 1 2; do
  ns=${NODES[$i]}; net=10.77.$((i + 1))
  root ip netns add "$ns"
  root ip link add "pmh$i" netns "$HUB" type veth peer name "pmn$i" netns "$ns"
  nsx "$HUB" ip addr add "$net.1/24" dev "pmh$i"
  nsx "$HUB" ip link set "pmh$i" up
  nsx "$ns" ip link set lo up
  nsx "$ns" ip addr add "$net.2/24" dev "pmn$i"
  nsx "$ns" ip link set "pmn$i" up
  nsx "$ns" ip route add default via "$net.1"
done
nsx "$HUB" sysctl -qw net.ipv4.ip_forward=0

if [ -n "$EXT_RELAY" ]; then
  relay=$EXT_RELAY
  root ip link add pm-up type veth peer name pmu netns "$HUB"
  root ip addr add "$UPLINK_NET.1/24" dev pm-up
  root ip link set pm-up up
  nsx "$HUB" ip addr add "$UPLINK_NET.2/24" dev pmu
  nsx "$HUB" ip link set pmu up
  nsx "$HUB" ip route add default via "$UPLINK_NET.1"
  nsx "$HUB" sysctl -qw net.ipv4.ip_forward=1
  # Nodes may go out through the hub but never to each other.
  nsx "$HUB" iptables -A FORWARD -i 'pmh+' -o 'pmh+' -j DROP
  root sysctl -qw net.ipv4.ip_forward=1
  # Docker (preinstalled on CI runners) defaults FORWARD to DROP.
  root iptables -I FORWARD -s 10.77.0.0/16 -j ACCEPT
  root iptables -I FORWARD -d 10.77.0.0/16 -j ACCEPT
  root iptables -t nat -A POSTROUTING -s 10.77.0.0/16 ! -d 10.77.0.0/16 -j MASQUERADE
  for ns in "${NODES[@]}"; do
    root mkdir -p "/etc/netns/$ns"
    printf 'nameserver 1.1.1.1\nnameserver 8.8.8.8\n' | root tee "/etc/netns/$ns/resolv.conf" >/dev/null
  done
else
  relay=ws://10.77.1.1:$PORT/ws
  root env POLYPH_RELAY_BIND="0.0.0.0:$PORT" POLYPH_RELAY_DATABASE=:memory: \
    ip netns exec "$HUB" "$target/debug/polyph-relay" >/dev/null &
  RELAY_PID=$!
  for _ in $(seq 50); do
    nsx pm-a curl -fsS "http://10.77.1.1:$PORT/healthz" >/dev/null 2>&1 && break
    sleep 0.2
  done
fi

# The three namespaces must not reach each other, but must reach the relay.
if nsx pm-a ping -c1 -W1 10.77.2.2 >/dev/null 2>&1 || nsx pm-b ping -c1 -W1 10.77.3.2 >/dev/null 2>&1; then
  echo "multinet: the namespaces can reach each other; the test would prove nothing" >&2
  exit 1
fi
if [ -z "$EXT_RELAY" ]; then
  nsx pm-c curl -fsS "http://10.77.1.1:$PORT/healthz" >/dev/null
fi

POLYPH_MULTINET=1 POLYPH_MULTINET_NS="${NODES[*]}" POLYPH_MULTINET_RELAY="$relay" \
  cargo test --locked -p polyph-testkit --test multinet -- --nocapture
