import Link from "next/link";

import { formatAge, formatAmount, formatDifficulty, shorten } from "@/lib/format";
import type { Block } from "@/lib/queries";

import { BlockStatus, Empty } from "./ui";

export function BlocksTable({ blocks }: { blocks: Block[] }) {
  if (blocks.length === 0) {
    return <Empty>No blocks found yet.</Empty>;
  }
  return (
    <table className="datagrid">
      <thead>
        <tr>
          <th>Name</th>
          <th className="right">Amount</th>
          <th className="right">Difficulty</th>
          <th className="right">Block</th>
          <th>Found by</th>
          <th className="right">Time</th>
          <th className="right">Status</th>
        </tr>
      </thead>
      <tbody>
        {blocks.map((block) => (
          <tr key={block.id}>
            <td>
              <b>{block.symbol}</b>
            </td>
            <td className="right">{formatAmount(block.reward, 2)}</td>
            <td className="right">{formatDifficulty(block.networkDifficulty)}</td>
            <td className="right" title={block.hash}>
              {block.height}
            </td>
            <td>
              <Link href={`/wallet/${block.finder}`} className="mono">
                {shorten(block.finder)}
              </Link>
              {(block.solo || block.party) && <span className="tag">{block.solo ? "solo" : "party"}</span>}
            </td>
            <td className="right">{formatAge(block.foundAt)}</td>
            <td className="right">
              <BlockStatus
                status={block.status}
                progress={
                  block.status === "pending" && block.blockMaturity !== null
                    ? `${block.confirmations}/${block.blockMaturity + 1}`
                    : undefined
                }
              />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
