import Link from "next/link";

import { formatAge, formatAmount, formatDifficulty, formatDuration, formatHashrate, timeToFind } from "@/lib/format";
import type { Coin } from "@/lib/queries";

import { Empty } from "./ui";

/** One row per coin; the algo links to the page that shows only that algorithm. */
export function CoinsTable({ coins }: { coins: Coin[] }) {
  if (coins.length === 0) {
    return <Empty>No coins are configured yet.</Empty>;
  }
  return (
    <table className="datagrid">
      <thead>
        <tr>
          <th>Name</th>
          <th>Algo</th>
          <th className="right">Port</th>
          <th className="right">Height</th>
          <th className="right">Difficulty</th>
          <th className="right">Net hash</th>
          <th className="right">Pool hash</th>
          <th className="right" title="Time to find: how long the pool needs on average for one block at its current hashrate">
            TTF
          </th>
          <th className="right">Miners</th>
          <th className="right">Blocks</th>
          <th className="right">Last block</th>
          <th className="right">Min. pay</th>
        </tr>
      </thead>
      <tbody>
        {coins.map((coin) => (
          <tr key={coin.symbol}>
            <td>
              <b>{coin.name}</b> ({coin.symbol})
            </td>
            <td>
              <Link href={`/algo/${coin.algo}`}>{coin.algo}</Link>
            </td>
            <td className="right">{coin.stratumPort ?? "–"}</td>
            <td className="right">{coin.height ?? "–"}</td>
            <td className="right">{formatDifficulty(coin.networkDifficulty)}</td>
            <td className="right">{coin.networkHashrate === null ? "–" : formatHashrate(coin.networkHashrate)}</td>
            <td className="right">{formatHashrate(coin.hashrate)}</td>
            <td className="right">{formatDuration(timeToFind(coin.networkDifficulty, coin.hashrate))}</td>
            <td className="right" title={`${coin.workers} workers`}>
              {coin.miners}
            </td>
            <td className="right">{coin.blocksFound}</td>
            <td className="right">{formatAge(coin.lastBlockAt)}</td>
            <td className="right">{coin.minPayout === null ? "–" : formatAmount(coin.minPayout, 2)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
