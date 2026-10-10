#!/usr/bin/env bash
# Turns this machine into a server of the mining pool.
#
#   curl -fsSL <pool url>/install.sh | sudo bash -s -- <pool url> <one-time code>
#
# The code comes from the pool's admin area and decides what this server runs.
# Nothing here gives the pool access to this machine: the server only ever calls out.
set -euo pipefail

POOL_URL="${1:?usage: install.sh <pool url> <one-time code>}"
CODE="${2:?usage: install.sh <pool url> <one-time code>}"
DIR="${POOL_NODE_DIR:-/opt/pool-node}"

if ! command -v docker >/dev/null 2>&1; then
  echo "Installing Docker..."
  curl -fsSL https://get.docker.com | sh
fi
docker compose version >/dev/null 2>&1 || { echo "Docker Compose is missing; install the docker-compose-plugin package." >&2; exit 1; }

mkdir -p "$DIR"
cd "$DIR"
umask 077

if [ -f .env ] && grep -q '^POOL_NODE_TOKEN=' .env; then
  echo "This server is already enrolled; keeping its token and refreshing its setup."
  # shellcheck disable=SC1091
  . ./.env
else
  echo "Enrolling with the pool..."
  REPLY="$(curl -fsS -X POST "$POOL_URL/api/node/enroll" -H 'content-type: application/json' -d "{\"code\":\"$CODE\"}")" \
    || { echo "The pool refused the code. It may be mistyped, already used or expired." >&2; exit 1; }
  POOL_NODE_TOKEN="$(printf '%s' "$REPLY" | sed -n 's/.*"token":"\([0-9a-f]*\)".*/\1/p')"
  [ -n "$POOL_NODE_TOKEN" ] || { echo "Unexpected answer from the pool." >&2; exit 1; }
  # The coin daemons' RPC password is made up here and never leaves this machine.
  RPC_PASSWORD="$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  printf 'POOL_NODE_TOKEN=%s\nRPC_PASSWORD=%s\n' "$POOL_NODE_TOKEN" "$RPC_PASSWORD" > .env
fi

curl -fsS "$POOL_URL/api/node/compose" -H "Authorization: Bearer $POOL_NODE_TOKEN" -o docker-compose.yml

echo "Starting services..."
docker compose pull --ignore-pull-failures >/dev/null 2>&1 || true
docker compose up -d

echo
echo "Done. This server will show up in the pool's admin area within a minute."
echo "Logs: cd $DIR && docker compose logs -f"
