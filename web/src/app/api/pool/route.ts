import { unitsToDecimal } from "@/lib/format";
import { getCoins } from "@/lib/queries";

export async function GET() {
  const coins = await getCoins();
  return Response.json({
    coins: coins.map(({ minPayout, ...coin }) => ({
      ...coin,
      minPayout: minPayout === null ? null : unitsToDecimal(minPayout),
    })),
  });
}
