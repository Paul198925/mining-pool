import Link from "next/link";

import { formatAge, formatAmount, shorten } from "@/lib/format";
import type { Block } from "@/lib/queries";

import { BlockStatus, Empty, td, th } from "./ui";

export function BlocksTable({ blocks }: { blocks: Block[] }) {
  if (blocks.length === 0) {
    return <Empty>No blocks found yet.</Empty>;
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full">
        <thead>
          <tr className="border-b border-border">
            <th className={th}>Coin</th>
            <th className={th}>Height</th>
            <th className={th}>Status</th>
            <th className={`${th} text-right`}>Reward</th>
            <th className={th}>Found by</th>
            <th className={th}>Hash</th>
            <th className={`${th} text-right`}>Found</th>
          </tr>
        </thead>
        <tbody>
          {blocks.map((block) => (
            <tr key={block.id} className="border-b border-border last:border-0">
              <td className={td}>{block.symbol}</td>
              <td className={`${td} font-mono`}>{block.height}</td>
              <td className={td}>
                <BlockStatus
                  status={block.status}
                  progress={
                    block.status === "pending" && block.blockMaturity !== null
                      ? `${block.confirmations}/${block.blockMaturity + 1}`
                      : undefined
                  }
                />
              </td>
              <td className={`${td} text-right font-mono`}>{formatAmount(block.reward, 2)}</td>
              <td className={`${td} font-mono`}>
                <Link href={`/miner/${block.finder}`} className="hover:text-accent">
                  {shorten(block.finder)}
                </Link>
                {block.solo && <span className="ml-2 rounded border border-border px-1 font-sans text-xs text-muted">solo</span>}
              </td>
              <td className={`${td} font-mono text-muted`} title={block.hash}>
                {shorten(block.hash)}
              </td>
              <td className={`${td} text-right whitespace-nowrap text-muted`}>{formatAge(block.foundAt)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
