/**
 * What each enrolled server should run and how the servers reach each other.
 *
 * Servers form a private WireGuard network, 10.77.0.0/24, where a server's last
 * address byte is its instance id. Coin daemons listen for RPC only on that network,
 * so a stratum service can use a daemon on any server, and nothing but the stratum
 * ports and WireGuard itself faces the internet.
 */

import { randomBytes } from "node:crypto";

import { COINS, type CoinSpec, coinSpec, image, network } from "./catalog";
import { db } from "./db";

export const WG_INTERFACE = "pool0";
export const WG_PORT = 51820;
const WG_NETWORK = "10.77.0";
/**
 * Where Docker puts its networks, as far as that cannot be the private network above:
 * 10.0.0.0 to 10.63.255.255 instead of all of 10.0.0.0/8.
 */
const LOCAL_DOCKER_NETWORKS = ["10.0.0.0/10", "172.16.0.0/12", "192.168.0.0/16"];

/** For local tests on one Docker host, where the servers are containers without WireGuard. */
export function meshEnabled(): boolean {
  return process.env.POOL_WIREGUARD !== "off";
}

export function wgAddress(id: number): string {
  return `${WG_NETWORK}.${id}`;
}

export type Server = {
  id: number;
  region: string;
  host: string;
  stratum: boolean;
  /** Coins this server runs a node for, without a wallet. */
  coins: string[];
  /** Coins whose pool wallet this server's node holds. */
  wallets: string[];
  /** pool-core runs on this machine. */
  core: boolean;
  wgPublicKey: string | null;
  configVersion: string;
};

const SERVER_COLUMNS = "id, region, host, stratum, coins, wallets, core, wg_public_key, config_version";

function toServer(r: Record<string, unknown>): Server {
  return {
    id: r.id as number,
    region: r.region as string,
    host: r.host as string,
    stratum: r.stratum as boolean,
    coins: r.coins as string[],
    wallets: r.wallets as string[],
    core: r.core as boolean,
    wgPublicKey: r.wg_public_key as string | null,
    configVersion: String(r.config_version),
  };
}

/** Stores the public key a server made for itself; the other servers pick it up within a minute. */
export async function setWireguardKey(server: Server, publicKey: string): Promise<void> {
  await db().query("UPDATE stratum_instances SET wg_public_key = $2 WHERE id = $1", [server.id, publicKey]);
}

export async function serverByToken(tokenHash: string): Promise<Server | null> {
  const { rows } = await db().query(`SELECT ${SERVER_COLUMNS} FROM stratum_instances WHERE token_hash = $1`, [tokenHash]);
  return rows[0] ? toServer(rows[0]) : null;
}

/** Every server that has completed enrollment. */
async function enrolledServers(): Promise<Server[]> {
  const { rows } = await db().query(
    `SELECT ${SERVER_COLUMNS} FROM stratum_instances WHERE token_hash IS NOT NULL ORDER BY id`,
  );
  return rows.map(toServer);
}

export type Algo = {
  name: string;
  port: number;
  startDifficulty: number;
  minDifficulty: number;
  maxDifficulty: number;
  shareTargetSecs: number;
};

export async function getAlgos(): Promise<Algo[]> {
  const { rows } = await db().query(
    "SELECT name, port, start_difficulty, min_difficulty, max_difficulty, share_target_secs FROM algos ORDER BY name",
  );
  return rows.map((r) => ({
    name: r.name,
    port: r.port,
    startDifficulty: r.start_difficulty,
    minDifficulty: r.min_difficulty,
    maxDifficulty: r.max_difficulty,
    shareTargetSecs: r.share_target_secs,
  }));
}

/**
 * Where each coin's block rewards go. pool-core publishes an address of the pool wallet
 * in the database; `POOL_ADDRESS_<SYMBOL>` in the web app's environment overrides it.
 * The admin area cannot change either, so nobody can redirect rewards from there.
 */
export async function poolAddresses(): Promise<Record<string, string>> {
  const { rows } = await db().query("SELECT symbol, pool_address FROM coins WHERE pool_address IS NOT NULL");
  const addresses: Record<string, string> = Object.fromEntries(rows.map((r) => [r.symbol, r.pool_address]));
  for (const coin of COINS) {
    const override = process.env[`POOL_ADDRESS_${coin.symbol}`];
    if (override) addresses[coin.symbol] = override;
  }
  return addresses;
}

/**
 * The RPC password of the wallet-less coin daemons on the servers. One value for the
 * whole pool, because a stratum service has to log in to daemons on other servers.
 * It guards daemons that hold no keys, and they are reachable only inside the network.
 */
export async function nodeRpcPassword(): Promise<string> {
  await db().query(
    "INSERT INTO pool_secrets (name, value) VALUES ('node_rpc_password', $1) ON CONFLICT (name) DO NOTHING",
    [randomBytes(24).toString("hex")],
  );
  const { rows } = await db().query("SELECT value FROM pool_secrets WHERE name = 'node_rpc_password'");
  return rows[0].value;
}

/**
 * The RPC password of nodes that hold a pool wallet. Only pool-core and the server the
 * wallet is on know it; the other servers cannot talk to a wallet at all.
 */
export async function walletRpcPassword(): Promise<string> {
  await db().query(
    "INSERT INTO pool_secrets (name, value) VALUES ('wallet_rpc_password', $1) ON CONFLICT (name) DO NOTHING",
    [randomBytes(24).toString("hex")],
  );
  const { rows } = await db().query("SELECT value FROM pool_secrets WHERE name = 'wallet_rpc_password'");
  return rows[0].value;
}

/** Whether the server runs a node for the coin, with or without the pool wallet. */
function runsNode(server: Server, symbol: string): boolean {
  return server.coins.includes(symbol) || server.wallets.includes(symbol);
}

/** The coins a server runs a node for; a wallet node serves miners as well. */
function nodeSymbols(server: Server): string[] {
  return [...new Set([...server.wallets, ...server.coins])];
}

/**
 * How a stratum service on `server` reaches a daemon of `coin`, or null if it cannot.
 * Its own node, wallet or not; otherwise a node of another server, but never another
 * server's wallet node: those take commands from pool-core only.
 */
function rpcUrl(server: Server, coin: CoinSpec, servers: Server[]): string | null {
  const port = coin.rpcPort[network()];
  if (!meshEnabled()) {
    return runsNode(server, coin.symbol) ? `http://${coin.service}:${port}` : null;
  }
  const host = runsNode(server, coin.symbol)
    ? server
    : servers.find(
        (other) =>
          other.coins.includes(coin.symbol) && !other.wallets.includes(coin.symbol) && other.wgPublicKey !== null,
      );
  return host ? `http://${wgAddress(host.id)}:${port}` : null;
}

/** The coins a stratum service on this server can offer right now. */
async function servedCoins(server: Server): Promise<{ coin: CoinSpec; url: string; address: string }[]> {
  if (!server.stratum) return [];
  const [servers, addresses, switches] = await Promise.all([
    enrolledServers(),
    poolAddresses(),
    // `wallet_seen`: pool-core has talked to the coin's wallet within the last hour.
    db().query("SELECT symbol, enabled, COALESCE(stats_at > now() - interval '1 hour', FALSE) AS wallet_seen FROM coins"),
  ]);
  // Without a wallet nobody could be paid, and the published reward address may be one
  // of a wallet that no longer exists: such a coin is not mined.
  const switchedOff = new Set(switches.rows.filter((r) => !r.enabled || !r.wallet_seen).map((r) => r.symbol));
  const usable = COINS.flatMap((coin) => {
    const url = rpcUrl(server, coin, servers);
    const address = addresses[coin.symbol];
    // A coin needs a daemon to mine on and a pool wallet to be paid into, and an admin can switch it off.
    return url && address && !switchedOff.has(coin.symbol) ? [{ coin, url, address }] : [];
  });
  // A merge-mined coin has no work of its own: without its parent it cannot be mined.
  return usable.filter(({ coin }) => !coin.auxOf || usable.some((other) => other.coin.symbol === coin.auxOf));
}

/** The pool's configuration for one server, in the shape the stratum service reads. */
export async function nodeConfig(server: Server) {
  const [served, algos, rpcPassword, walletPassword] = await Promise.all([
    servedCoins(server),
    getAlgos(),
    nodeRpcPassword(),
    walletRpcPassword(),
  ]);
  const used = new Set(served.map(({ coin }) => coin.algo));
  return {
    database: { url: "" },
    instance: { id: server.id, region: server.region, host: server.host },
    stratum: algos
      .filter((algo) => used.has(algo.name))
      .map((algo) => ({
        listen: `0.0.0.0:${algo.port}`,
        algo: algo.name,
        start_difficulty: algo.startDifficulty,
        min_difficulty: algo.minDifficulty,
        max_difficulty: algo.maxDifficulty,
        share_target_secs: algo.shareTargetSecs,
      })),
    // Accounting happens centrally; the stratum service only needs the section to exist.
    accounting: { pplns_factor: 0, pool_fee_percent: 0, solo_fee_percent: 0, payout_interval_secs: 0 },
    coins: served.map(({ coin, url, address }) => ({
      symbol: coin.symbol,
      name: coin.name,
      algo: coin.algo,
      chain: coin.chain,
      template_rules: coin.templateRules ?? [],
      aux_of: coin.auxOf ?? null,
      rpc_url: url,
      rpc_user: "pool",
      // Its own wallet node has the wallet password, which this server holds anyway.
      rpc_password: server.wallets.includes(coin.symbol) ? walletPassword : rpcPassword,
      pool_address: address,
      block_maturity: coin.blockMaturity,
      min_payout: coin.minPayout,
    })),
  };
}

/** The symbols a stratum service on this server may report shares for. */
export async function servedSymbols(server: Server): Promise<string[]> {
  return (await servedCoins(server)).map(({ coin }) => coin.symbol);
}

/**
 * The docker-compose file for one server. Daemons run without a wallet. With the
 * network on, they use the host's network stack and accept RPC only on the server's
 * WireGuard address; the stratum service reaches local and remote daemons the same way.
 */
export async function nodeCompose(server: Server, poolUrl: string): Promise<string> {
  const net = network();
  const mesh = meshEnabled();
  const coins = nodeSymbols(server).flatMap((symbol) => coinSpec(symbol) ?? []);
  const [algos, servers] = await Promise.all([getAlgos(), enrolledServers()]);
  const coreHost = servers.find((other) => other.core);
  const lines = ["# Generated by the pool. Run the update command from the admin area to change it.", "name: pool-node", "services:"];

  if (server.stratum) {
    lines.push(
      "  stratum:",
      `    image: ${image("stratum")}`,
      "    restart: unless-stopped",
      "    environment:",
      `      POOL_API_URL: ${JSON.stringify(poolUrl)}`,
      "      POOL_NODE_TOKEN: ${POOL_NODE_TOKEN}",
      // On the host's network stack it reaches daemons on every server of the private
      // network; otherwise one published port per algorithm.
      ...(mesh ? ["    network_mode: host"] : ["    ports:", ...algos.map((algo) => `      - "${algo.port}:${algo.port}"`)]),
    );
    if (!mesh && coins.length > 0) {
      lines.push("    depends_on:", ...coins.map((coin) => `      - ${coin.service}`));
    }
  }

  for (const coin of coins) {
    const wallet = server.wallets.includes(coin.symbol);
    // A wallet node takes commands from pool-core's machine and from this server, nobody else.
    const allowed = wallet
      ? [
          `${wgAddress(server.id)}/32`,
          ...(coreHost ? [`${wgAddress(coreHost.id)}/32`] : []),
          // On the pool host itself pool-core does not come through the private network: it
          // calls from its container, with an address of a local Docker network.
          ...(server.core ? LOCAL_DOCKER_NETWORKS : []),
        ].map((range) => `      - -rpcallowip=${range}`)
      : [`      - -rpcallowip=${WG_NETWORK}.0/24`];
    lines.push(
      `  ${coin.service}:`,
      `    image: ${image(coin.service)}`,
      "    restart: unless-stopped",
      // Time to write its state to disk before it is stopped.
      "    stop_grace_period: 5m",
      ...(mesh ? ["    network_mode: host"] : []),
      // The images create a wallet of this name on first start where the daemon does not by itself.
      ...(wallet ? ["    environment:", "      POOL_WALLET: pool"] : []),
      "    command:",
      ...(net === "regtest" ? ["      - -regtest"] : []),
      ...(coin.nodeArgs?.[net] ?? []).map((arg) => `      - ${arg}`),
      "      - -server",
      ...(wallet ? [] : ["      - -disablewallet"]),
      // Mining needs the current chain state, not the history: keep about 4 GB of blocks.
      "      - -prune=4000",
      // Not reachable from the internet: bound to the private network, or unpublished.
      ...(mesh
        ? [`      - -rpcbind=${wgAddress(server.id)}`, "      - -rpcbind=127.0.0.1", ...allowed]
        : [
            "      - -rpcbind=0.0.0.0",
            "      - -rpcallowip=10.0.0.0/8",
            "      - -rpcallowip=172.16.0.0/12",
            "      - -rpcallowip=192.168.0.0/16",
          ]),
      "      - -rpcallowip=127.0.0.0/8",
      `      - -rpcport=${coin.rpcPort[net]}`,
      "      - -rpcuser=pool",
      `      - -rpcpassword=\${${wallet ? "WALLET_RPC_PASSWORD" : "RPC_PASSWORD"}}`,
      "    volumes:",
      `      - ${coin.service}:/data`,
    );
  }
  if (coins.length > 0) {
    lines.push("volumes:", ...coins.map((coin) => `  ${coin.service}:`));
  }
  return `${lines.join("\n")}\n`;
}

/**
 * The WireGuard configuration for one server: itself plus every other enrolled server.
 * `__PRIVATE_KEY__` is filled in on the server, which is the only place its key exists.
 */
export async function wireguardConfig(server: Server): Promise<string> {
  const peers = (await enrolledServers()).filter((other) => other.id !== server.id && other.wgPublicKey);
  const lines = [
    "# Generated by the pool; rewritten automatically when servers join or leave.",
    "[Interface]",
    `Address = ${wgAddress(server.id)}/24`,
    `ListenPort = ${WG_PORT}`,
    "PrivateKey = __PRIVATE_KEY__",
  ];
  for (const peer of peers) {
    lines.push(
      "",
      "[Peer]",
      `# ${peer.region} (server ${peer.id})`,
      `PublicKey = ${peer.wgPublicKey}`,
      `Endpoint = ${peer.host}:${WG_PORT}`,
      `AllowedIPs = ${wgAddress(peer.id)}/32`,
      "PersistentKeepalive = 25",
    );
  }
  return `${lines.join("\n")}\n`;
}
