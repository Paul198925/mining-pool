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
  chain: "bitcoin" | "xaya";
  /** Image name suffix and compose service name. */
  service: string;
  rpcPort: Record<Network, number>;
  /** Rule sets the node wants named in block-template requests. */
  templateRules?: string[];
  blockMaturity: number;
  /** Default until an admin sets another value. */
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
];

export function coinSpec(symbol: string): CoinSpec | undefined {
  return COINS.find((coin) => coin.symbol === symbol);
}
