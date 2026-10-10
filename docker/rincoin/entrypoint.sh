#!/bin/sh
# Starts the daemon. With POOL_WALLET set, makes sure a wallet of that name exists and
# loads it. Only the node that holds the pool's own wallet uses this; servers that just
# mine run without a wallet.
set -e
if [ -n "$POOL_WALLET" ]; then
  chain=""
  for arg in "$@"; do
    [ "$arg" = "-regtest" ] && chain="-regtest"
  done
  # Creating fails harmlessly when the wallet is already there.
  rincoin-wallet -datadir=/data $chain -wallet="$POOL_WALLET" create >/dev/null 2>&1 \
    || rincoin-wallet -datadir=/data $chain -wallet="$POOL_WALLET" -descriptors create >/dev/null 2>&1 \
    || true
  set -- "-wallet=$POOL_WALLET" "$@"
fi
exec rincoind -datadir=/data -printtoconsole "$@"
