/**
 * The coins this pool's code knows how to run: which image, which ports, which block
 * layout. Everything an admin can change (fees, stratum ports, which server runs what)
 * lives in the database instead; see `mesh.ts`.
 */

export type Network = "main" | "regtest";

export function network(): Network {
  return process.env.POOL_NETWORK === "regtest" ? "regtest" : "main";
}

/** Image names are `<prefix>-<name>`, e.g. `ghcr.io/acme/pool-stratum`. */
export function image(name: string): string {
  return `${process.env.POOL_IMAGE_PREFIX ?? "pool"}-${name}`;
}

export type CoinSpec = {
  symbol: string;
  name: string;
  algo: string;
  chain: "bitcoin" | "xaya" | "ravencoin";
  /** Image name suffix and compose service name. */
  service: string;
  rpcPort: Record<Network, number>;
  /** Rule sets the node wants named in block-template requests. */
  templateRules?: string[];
  /** Symbol of the coin this one is merge-mined with; it cannot be mined on its own then. */
  auxOf?: string;
  /** Extra daemon arguments a network needs. */
  nodeArgs?: Partial<Record<Network, string[]>>;
  blockMaturity: number;
  /** Default until an admin sets another value. */
  minPayout: number;
};

export const COINS: CoinSpec[] = [
  {
    symbol: "LTC",
    name: "Litecoin",
    algo: "scrypt",
    chain: "bitcoin",
    service: "litecoin",
    rpcPort: { main: 9332, regtest: 19443 },
    templateRules: ["mweb", "segwit"],
    // MWEB needs a peg-in by hand before it can activate on regtest.
    nodeArgs: { regtest: ["-vbparams=mweb:-2:0"] },
    blockMaturity: 100,
    minPayout: 0.01,
  },
  {
    symbol: "DOGE",
    name: "Dogecoin",
    algo: "scrypt",
    chain: "bitcoin",
    auxOf: "LTC",
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
  {
    symbol: "RIN",
    name: "Rincoin",
    algo: "rinhash",
    chain: "bitcoin",
    service: "rincoin",
    rpcPort: { main: 9556, regtest: 29556 },
    templateRules: ["mweb", "segwit"],
    blockMaturity: 100,
    minPayout: 1,
  },
  {
    symbol: "RVN",
    name: "Ravencoin",
    algo: "kawpow",
    chain: "ravencoin",
    service: "ravencoin",
    rpcPort: { main: 8766, regtest: 18766 },
    // On regtest KawPoW only starts in 2083, and a lone node has no peers.
    nodeArgs: { regtest: ["-mocktime=3582830200", "-bypassdownload"] },
    blockMaturity: 100,
    minPayout: 10,
  },
];

export function coinSpec(symbol: string): CoinSpec | undefined {
  return COINS.find((coin) => coin.symbol === symbol);
}

/**
 * What miners can connect for: coins with a stratum port that are not merge-mined with
 * another one. A coin that has one riding along names it as `merged`.
 */
export function mineableCoins(
  coins: { symbol: string; name: string; algo: string; stratumPort: number | null }[],
): { symbol: string; name: string; algo: string; port: number; merged?: string }[] {
  return coins.flatMap((coin) => {
    if (coin.stratumPort === null || coinSpec(coin.symbol)?.auxOf) return [];
    const merged = COINS.find((other) => other.auxOf === coin.symbol && coins.some((c) => c.symbol === other.symbol));
    return [{ symbol: coin.symbol, name: coin.name, algo: coin.algo, port: coin.stratumPort, merged: merged?.symbol }];
  });
}
