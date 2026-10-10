/**
 * What a self-enrolling server needs to know to run a coin: which image, how to start
 * the daemon, and on which stratum port its algorithm is served. The pool's node API
 * turns this into the server's configuration and its docker-compose file.
 */

import { db } from "./db";

export type Network = "main" | "regtest";

export function network(): Network {
  return process.env.POOL_NETWORK === "regtest" ? "regtest" : "main";
}

/** Image names are `<prefix>-<name>`, e.g. `ghcr.io/acme/pool-stratum`. */
export function image(name: string): string {
  return `${process.env.POOL_IMAGE_PREFIX ?? "pool"}-${name}`;
}

export type AlgoSpec = {
  port: number;
  startDifficulty: number;
  minDifficulty: number;
  maxDifficulty: number;
};

export const ALGOS: Record<string, AlgoSpec> = {
  scrypt: { port: 3333, startDifficulty: 8192, minDifficulty: 16, maxDifficulty: 1e8 },
  "neoscrypt-xaya": { port: 3334, startDifficulty: 4, minDifficulty: 0.01, maxDifficulty: 1e6 },
  // RinHash miners take stratum difficulty at face value, and a CPU does some 25 kH/s per thread.
  rinhash: { port: 3335, startDifficulty: 0.0002, minDifficulty: 0.000001, maxDifficulty: 1e4 },
};

/** Regtest difficulty is trivial, so real-world share difficulties would never be met. */
const REGTEST_DIFFICULTY = { startDifficulty: 0.01, minDifficulty: 0.0001, maxDifficulty: 1e6 };

export function algoSpec(algo: string): AlgoSpec {
  return network() === "regtest" ? { ...ALGOS[algo], ...REGTEST_DIFFICULTY } : ALGOS[algo];
}

export type CoinSpec = {
  symbol: string;
  name: string;
  algo: string;
  chain: "bitcoin" | "xaya";
  /** Image name suffix and compose service name. */
  service: string;
  rpcPort: Record<Network, number>;
  /** Rule sets the node wants named in block-template requests. */
  templateRules?: string[];
  blockMaturity: number;
  minPayout: number;
};

export const COINS: CoinSpec[] = [
  {
    symbol: "DOGE",
    name: "Dogecoin",
    algo: "scrypt",
    chain: "bitcoin",
    service: "dogecoin",
    rpcPort: { main: 22555, regtest: 18332 },
    blockMaturity: 240,
    minPayout: 50,
  },
  {
    symbol: "ROD",
    name: "SpaceXpanse",
    algo: "neoscrypt-xaya",
    chain: "xaya",
    service: "spacexpanse",
    rpcPort: { main: 11999, regtest: 18499 },
    blockMaturity: 120,
    minPayout: 50,
  },
];

COINS.push({
  symbol: "RIN",
  name: "Rincoin",
  algo: "rinhash",
  chain: "bitcoin",
  service: "rincoin",
  rpcPort: { main: 9556, regtest: 29556 },
  templateRules: ["mweb", "segwit"],
  blockMaturity: 100,
  minPayout: 1,
});

export function coinSpec(symbol: string): CoinSpec | undefined {
  return COINS.find((coin) => coin.symbol === symbol);
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

/** The pool's configuration for one server, in the shape the stratum service reads. */
export function nodeConfig(
  node: { id: number; region: string; host: string; coins: string[] },
  addresses: Record<string, string>,
) {
  const coins = node.coins.flatMap((symbol) => coinSpec(symbol) ?? []);
  const algos = [...new Set(coins.map((coin) => coin.algo))];
  return {
    database: { url: "" },
    instance: { id: node.id, region: node.region, host: node.host },
    stratum: algos.map((algo) => {
      const spec = algoSpec(algo);
      return {
        listen: `0.0.0.0:${spec.port}`,
        algo,
        start_difficulty: spec.startDifficulty,
        min_difficulty: spec.minDifficulty,
        max_difficulty: spec.maxDifficulty,
        share_target_secs: 15,
      };
    }),
    // Accounting happens centrally; the stratum service only needs the section to exist.
    accounting: { pplns_factor: 0, pool_fee_percent: 0, solo_fee_percent: 0, payout_interval_secs: 0 },
    coins: coins.map((coin) => ({
      symbol: coin.symbol,
      name: coin.name,
      algo: coin.algo,
      chain: coin.chain,
      template_rules: coin.templateRules ?? [],
      rpc_url: `http://${coin.service}:${coin.rpcPort[network()]}`,
      rpc_user: "pool",
      pool_address: addresses[coin.symbol] ?? null,
      block_maturity: coin.blockMaturity,
      min_payout: coin.minPayout,
    })),
  };
}

/**
 * The docker-compose file for one server. Daemons run without a wallet and their RPC is
 * only reachable inside the compose network; `RPC_PASSWORD` is generated on the server
 * by the installer and never leaves it.
 */
export function nodeCompose(node: { coins: string[] }, poolUrl: string): string {
  const net = network();
  const coins = node.coins.flatMap((symbol) => coinSpec(symbol) ?? []);
  const ports = [...new Set(coins.map((coin) => ALGOS[coin.algo].port))];
  const lines = [
    "# Generated by the pool. Re-run the installer to change it.",
    "name: pool-node",
    "services:",
    "  stratum:",
    `    image: ${image("stratum")}`,
    "    restart: unless-stopped",
    "    environment:",
    `      POOL_API_URL: ${JSON.stringify(poolUrl)}`,
    "      POOL_NODE_TOKEN: ${POOL_NODE_TOKEN}",
    ...coins.map((coin) => `      POOL_RPC_PASSWORD_${coin.symbol}: \${RPC_PASSWORD}`),
    "    ports:",
    ...ports.map((port) => `      - "${port}:${port}"`),
    "    depends_on:",
    ...coins.map((coin) => `      - ${coin.service}`),
  ];
  for (const coin of coins) {
    lines.push(
      `  ${coin.service}:`,
      `    image: ${image(coin.service)}`,
      "    restart: unless-stopped",
      "    command:",
      ...(net === "regtest" ? ["      - -regtest"] : []),
      "      - -server",
      "      - -disablewallet",
      "      - -rpcbind=0.0.0.0",
      "      - -rpcallowip=0.0.0.0/0",
      `      - -rpcport=${coin.rpcPort[net]}`,
      "      - -rpcuser=pool",
      "      - -rpcpassword=${RPC_PASSWORD}",
      "    volumes:",
      `      - ${coin.service}:/data`,
    );
  }
  lines.push("volumes:", ...coins.map((coin) => `  ${coin.service}:`));
  return `${lines.join("\n")}\n`;
}
