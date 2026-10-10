import { unitsToDecimal } from "@/lib/format";
import { getCoins, getStratumServers } from "@/lib/queries";

export async function GET() {
  const [coins, servers] = await Promise.all([getCoins(), getStratumServers()]);
  return Response.json({
    servers,
    coins: coins.map(({ minPayout, ...coin }) => ({
      ...coin,
      minPayout: minPayout === null ? null : unitsToDecimal(minPayout),
    })),
  });
}
