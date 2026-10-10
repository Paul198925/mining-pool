#!/usr/bin/env bash
# Turns this machine into a server of the mining pool, or updates one.
#
#   New server:  curl -fsSL <pool url>/install.sh | sudo bash -s -- <pool url> <one-time code>
#   Update:      curl -fsSL <pool url>/install.sh | sudo bash -s -- <pool url>
#
# The code comes from the pool's admin area, which also decides what this server runs:
# a stratum service, coin daemons, or both. Run the update command after changing that.
# Nothing here gives the pool access to this machine: the server only ever calls out.
set -euo pipefail

USAGE="usage: install.sh <pool url> [one-time code]"
POOL_URL="${1:?$USAGE}"
CODE="${2:-}"
DIR="${POOL_NODE_DIR:-/opt/pool-node}"
WG_IF=pool0
WG_PORT=51820

[ "$(id -u)" = 0 ] || { echo "Run this as root." >&2; exit 1; }

if ! command -v docker >/dev/null 2>&1; then
  echo "Installing Docker..."
  curl -fsSL https://get.docker.com | sh
fi
docker compose version >/dev/null 2>&1 || { echo "Docker Compose is missing; install the docker-compose-plugin package." >&2; exit 1; }

mkdir -p "$DIR"
cd "$DIR"
umask 077

# --- enrollment -----------------------------------------------------------------------
POOL_NODE_TOKEN=""
if [ -f token ]; then
  POOL_NODE_TOKEN="$(cat token)"
elif [ -f .env ] && grep -q '^POOL_NODE_TOKEN=' .env; then
  POOL_NODE_TOKEN="$(sed -n 's/^POOL_NODE_TOKEN=//p' .env)"
fi
# A token from an earlier enrollment may belong to a server that was removed since.
if [ -n "$POOL_NODE_TOKEN" ]; then
  KNOWN="$(curl -sS -o /dev/null -w '%{http_code}' "$POOL_URL/api/node/env" -H "Authorization: Bearer $POOL_NODE_TOKEN")"
  if [ "$KNOWN" = 401 ]; then
    if [ -z "$CODE" ]; then
      echo "The pool does not know this server any more. Add it again in the admin area and run the command with the new code." >&2
      exit 1
    fi
    echo "This machine was enrolled before, but the pool no longer knows that server. Enrolling it anew."
    POOL_NODE_TOKEN=""
  elif [ "$KNOWN" != 200 ]; then
    echo "Could not reach the pool (HTTP $KNOWN)." >&2
    exit 1
  elif [ -n "$CODE" ]; then
    echo "This server is already enrolled; the code is not needed and stays unused."
  fi
fi
if [ -z "$POOL_NODE_TOKEN" ]; then
  [ -n "$CODE" ] || { echo "This server is not enrolled yet; add it in the pool's admin area to get a one-time code." >&2; exit 1; }
  echo "Enrolling with the pool..."
  REPLY="$(curl -fsS -X POST "$POOL_URL/api/node/enroll" -H 'content-type: application/json' -d "{\"code\":\"$CODE\"}")" \
    || { echo "The pool refused the code. It may be mistyped, already used or expired." >&2; exit 1; }
  POOL_NODE_TOKEN="$(printf '%s' "$REPLY" | sed -n 's/.*"token":"\([0-9a-f]*\)".*/\1/p')"
  [ -n "$POOL_NODE_TOKEN" ] || { echo "Unexpected answer from the pool." >&2; exit 1; }
fi
printf '%s' "$POOL_NODE_TOKEN" > token
AUTH="Authorization: Bearer $POOL_NODE_TOKEN"
printf '%s' "$POOL_URL" > pool-url

# --- settings -------------------------------------------------------------------------
curl -fsS "$POOL_URL/api/node/env" -H "$AUTH" -o env.new \
  || { echo "The pool does not know this server any more. Was it removed in the admin area?" >&2; exit 1; }
{ printf 'POOL_NODE_TOKEN=%s\n' "$POOL_NODE_TOKEN"; cat env.new; } > .env
rm -f env.new
STRATUM_PORTS="$(sed -n 's/^STRATUM_PORTS="\(.*\)"$/\1/p' .env)"

# --- private network ------------------------------------------------------------------
# Fetches this server's WireGuard configuration and applies it if it changed.
cat > sync.sh <<'SYNC'
#!/usr/bin/env bash
# Run every minute: keeps the list of the pool's other servers current and tells the pool we are alive.
set -euo pipefail
cd "$(dirname "$0")"
POOL_URL="$(cat pool-url)"
AUTH="Authorization: Bearer $(cat token)"
WG_IF=pool0
curl -fsS -m 20 -X POST "$POOL_URL/api/node/heartbeat" -H "$AUTH" -H 'content-type: application/json' -d '{}' >/dev/null || true
[ -f wg-private.key ] || exit 0
curl -fsS -m 20 "$POOL_URL/api/node/wireguard" -H "$AUTH" -o wg.new
[ -s wg.new ] || exit 0
grep -q '^\[Interface\]' wg.new || { echo "unexpected answer from the pool" >&2; exit 1; }
sed -i "s|__PRIVATE_KEY__|$(cat wg-private.key)|" wg.new
if ! cmp -s wg.new "/etc/wireguard/$WG_IF.conf"; then
  install -m 600 wg.new "/etc/wireguard/$WG_IF.conf"
  if ip link show "$WG_IF" >/dev/null 2>&1; then
    wg-quick strip "$WG_IF" > wg.stripped
    wg syncconf "$WG_IF" wg.stripped
    rm -f wg.stripped
  else
    wg-quick up "$WG_IF"
  fi
fi
rm -f wg.new
SYNC
chmod 700 sync.sh

MESH="$(curl -fsS -o /dev/null -w '%{http_code}' "$POOL_URL/api/node/wireguard" -H "$AUTH")"
if [ "$MESH" = 200 ]; then
  if ! command -v wg >/dev/null 2>&1 || ! command -v wg-quick >/dev/null 2>&1; then
    echo "Installing WireGuard..."
    if command -v apt-get >/dev/null 2>&1; then
      apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq wireguard-tools iproute2 >/dev/null
    elif command -v dnf >/dev/null 2>&1; then
      dnf install -y -q wireguard-tools iproute
    elif command -v yum >/dev/null 2>&1; then
      yum install -y -q wireguard-tools iproute
    elif command -v apk >/dev/null 2>&1; then
      apk add --quiet wireguard-tools iproute2
    else
      echo "Install the wireguard-tools package and run this again." >&2; exit 1
    fi
  fi
  mkdir -p /etc/wireguard
  if [ ! -f wg-private.key ]; then
    # The private key is made here and never leaves this machine.
    wg genkey > wg-private.key
  fi
  PUBLIC_KEY="$(wg pubkey < wg-private.key)"
  curl -fsS -X POST "$POOL_URL/api/node/wireguard" -H "$AUTH" -H 'content-type: application/json' \
    -d "{\"public_key\":\"$PUBLIC_KEY\"}" >/dev/null
  ./sync.sh
  if command -v ufw >/dev/null 2>&1 && ufw status | grep -q '^Status: active'; then
    echo "Opening the firewall (ufw) for the private network and the stratum ports..."
    ufw allow "$WG_PORT/udp" >/dev/null
    ufw allow in on "$WG_IF" >/dev/null
    for port in $STRATUM_PORTS; do ufw allow "$port/tcp" >/dev/null; done
  fi
elif [ "$MESH" != 204 ]; then
  echo "Could not ask the pool about the private network (HTTP $MESH)." >&2; exit 1
fi

if [ -d /run/systemd/system ]; then
  cat > /etc/systemd/system/pool-node-sync.service <<UNIT
[Unit]
Description=Mining pool server: peer list and heartbeat
After=network-online.target
[Service]
Type=oneshot
ExecStart=$DIR/sync.sh
UNIT
  cat > /etc/systemd/system/pool-node-sync.timer <<UNIT
[Unit]
Description=Mining pool server: peer list and heartbeat, every minute
[Timer]
OnBootSec=30
OnUnitActiveSec=60
[Install]
WantedBy=timers.target
UNIT
  systemctl daemon-reload
  systemctl enable --now pool-node-sync.timer >/dev/null 2>&1
  [ "$MESH" = 200 ] && systemctl enable "wg-quick@$WG_IF" >/dev/null 2>&1 || true
else
  echo "No systemd here: run $DIR/sync.sh every minute yourself, e.g. from cron."
fi

# --- services -------------------------------------------------------------------------
curl -fsS "$POOL_URL/api/node/compose" -H "$AUTH" -D compose.headers -o docker-compose.yml
VERSION="$(tr -d '\r' < compose.headers | sed -n 's/^[Xx]-[Pp]ool-[Cc]onfig-[Vv]ersion: *//p')"
rm -f compose.headers

echo "Starting services..."
docker compose pull --ignore-pull-failures >/dev/null 2>&1 || true
if grep -q '^  [a-z]' docker-compose.yml; then
  docker compose up -d --remove-orphans
else
  # Nothing is assigned to this server any more.
  docker compose down --remove-orphans 2>/dev/null || true
fi

curl -fsS -X POST "$POOL_URL/api/node/heartbeat" -H "$AUTH" -H 'content-type: application/json' \
  -d "{\"applied_version\":\"$VERSION\"}" >/dev/null

echo
echo "Done. This server shows up in the pool's admin area within a minute."
echo "Logs: cd $DIR && docker compose logs -f"
