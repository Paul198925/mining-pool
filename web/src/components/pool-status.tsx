import Link from "next/link";

import { formatHashrate } from "@/lib/format";
import { getCoins } from "@/lib/queries";

import { Box, Empty } from "./ui";

/** Distinct values of one column within an algo, e.g. "3334, 3335". */
function distinct<T>(values: (T | null)[], format: (value: T) => string): string {
  const present = values.filter((value): value is T => value !== null).map(format);
  return present.length === 0 ? "–" : [...new Set(present)].join(", ");
}

/** One row per algorithm, as in YiiMP; the Pool page lists the individual coins. */
export async function PoolStatus() {
  const coins = await getCoins();
  const algos = [...new Set(coins.map((coin) => coin.algo))].sort();
  return (
    <Box title="Pool Status">
      {coins.length === 0 ? (
        <Empty>No coins are configured yet.</Empty>
      ) : (
        <table className="datagrid">
          <thead>
            <tr>
              <th>Algo</th>
              <th className="right">Port</th>
              <th className="right">Coins</th>
              <th className="right">Miners</th>
              <th className="right">Hashrate</th>
              <th className="right">Fees</th>
              <th className="right">Solo</th>
            </tr>
          </thead>
          <tbody>
            {algos.map((algo) => {
              const members = coins.filter((coin) => coin.algo === algo);
              return (
                <tr key={algo}>
                  <td>
                    <b>
                      <Link href={`/algo/${algo}`}>{algo}</Link>
                    </b>
                  </td>
                  <td className="right">{distinct(members.map((coin) => coin.stratumPort), String)}</td>
                  <td className="right">{members.length}</td>
                  <td className="right">{members.reduce((sum, coin) => sum + coin.miners, 0)}</td>
                  <td className="right">{formatHashrate(members.reduce((sum, coin) => sum + coin.hashrate, 0))}</td>
                  <td className="right">{distinct(members.map((coin) => coin.poolFeePercent), (fee) => `${fee}%`)}</td>
                  <td className="right">{distinct(members.map((coin) => coin.soloFeePercent), (fee) => `${fee}%`)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </Box>
  );
}
