import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../../target/types/raydium_clmm";
import { PDAUtils } from "./pda";
import {
  Raydium,
  PoolUtils,
  TickArrayUtil,
  getPdaTickArrayAddress,
} from "@raydium-io/raydium-sdk-v2";
import { AccountMeta, Connection, Keypair, PublicKey } from "@solana/web3.js";


export function getTickArrayAddressByTick(
  programId: PublicKey,
  poolId: PublicKey,
  tickIndex: number,
  tickSpacing: number
): PublicKey {
  const startIndex = TickArrayUtil.getTickArrayStartIndex(tickIndex, tickSpacing);
  return getPdaTickArrayAddress(programId, poolId, startIndex).publicKey;
}

// Helper: given a tick, compute its tick array and return whether the bitmap bit is 0 or 1
export async function getTickArrayBitmapBit(
  program: Program<RaydiumClmm>,
  pda: PDAUtils,
  poolStateKey: anchor.web3.PublicKey,
  tickIndex: number,
  tickSpacing: number
): Promise<0 | 1> {
  const ticksInArray = 60 * tickSpacing;
  let tickArrayStartIndex = Math.floor(tickIndex / ticksInArray);
  if (tickIndex < 0 && tickIndex % ticksInArray !== 0) {
    tickArrayStartIndex = tickArrayStartIndex - 1;
  }
  tickArrayStartIndex = tickArrayStartIndex * ticksInArray;

  const maxTickBoundary = ticksInArray * 512;
  const minTickBoundary = -maxTickBoundary;
  const isOverflowDefaultBitmap =
    tickArrayStartIndex >= maxTickBoundary ||
    tickArrayStartIndex < minTickBoundary;

  if (isOverflowDefaultBitmap) {
    const [tickArrayBitmap] = await pda.getTickArrayBitmapPDA(poolStateKey);
    try {
      const extensionData =
        await program.account.tickArrayBitmapExtension.fetch(tickArrayBitmap);
      const ticksInOneBitmap = ticksInArray * 512;
      let offset =
        Math.floor(Math.abs(tickArrayStartIndex) / ticksInOneBitmap) - 1;
      if (
        tickArrayStartIndex < 0 &&
        Math.abs(tickArrayStartIndex) % ticksInOneBitmap === 0
      ) {
        offset = offset - 1;
      }

      const tickArrayOffsetInBitmap =
        (Math.abs(tickArrayStartIndex) % ticksInOneBitmap) / ticksInArray;

      const bitmapArray =
        tickArrayStartIndex >= 0
          ? extensionData.positiveTickArrayBitmap
          : extensionData.negativeTickArrayBitmap;
      const tickArrayOffsetInBitmapValue = bitmapArray[offset] || [];
      const u64Index = Math.floor(tickArrayOffsetInBitmap / 64);
      const bitIndex = tickArrayOffsetInBitmap % 64;
      const word = tickArrayOffsetInBitmapValue[u64Index] || new anchor.BN(0);
      const mask = new anchor.BN(1).shln(bitIndex);
      const bitSet = !word.and(mask).eq(new anchor.BN(0));
      return bitSet ? 1 : 0;
    } catch (_e) {
      return 0;
    }
  } else {
    const poolStateAfter = await program.account.poolState.fetch(poolStateKey);
    let compressed = Math.floor(tickArrayStartIndex / ticksInArray) + 512;
    if (tickArrayStartIndex < 0 && tickArrayStartIndex % ticksInArray !== 0) {
      compressed = compressed - 1;
    }
    const bitPos = Math.abs(compressed);
    const u64Index = Math.floor(bitPos / 64);
    const bitIndex = bitPos % 64;
    if (u64Index >= 16) return 0;
    const word = poolStateAfter.tickArrayBitmap[u64Index] || new anchor.BN(0);
    const mask = new anchor.BN(1).shln(bitIndex);
    const bitSet = !word.and(mask).eq(new anchor.BN(0));
    return bitSet ? 1 : 0;
  }
}

export async function getTickStateByTick(
  program: Program<RaydiumClmm>,
  poolStateKey: anchor.web3.PublicKey,
  tickIndex: number,
  tickSpacing: number
) {
  const tickArrayStartIndex = TickArrayUtil.getTickArrayStartIndex(
    tickIndex,
    tickSpacing
  );

  const tickArrayAddress = getTickArrayAddressByTick(
    program.programId,
    poolStateKey,
    tickIndex,
    tickSpacing
  );

  // Fetch tick array state
  const tickArrayState = await program.account.tickArrayState.fetch(
    tickArrayAddress
  );
  // Calculate tick offset in array, following the contract implementation of get_tick_offset_in_array
  const offsetInArray = Math.floor(
    (tickIndex - tickArrayStartIndex) / tickSpacing
  );

  // Return the corresponding tick state
  return tickArrayState.ticks[offsetInArray];
}

export async function measureComputeUnits(
  connection: Connection,
  txSignature: string
): Promise<number | null> {
  try {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const tx = await connection.getTransaction(txSignature, {
      commitment: "confirmed",
      maxSupportedTransactionVersion: 0,
    });

    if (!tx || !tx.meta) {
      return null;
    }

    // Get compute units from transaction metadata
    const computeUnits = tx.meta.computeUnitsConsumed;
    return computeUnits || null;
  } catch (error) {
    console.error("Error getting transaction:", error);
    return null;
  }
}


export function getTickArrayRemainingAccounts(
  programId: anchor.web3.PublicKey,
  poolState: anchor.web3.PublicKey,
  tickIndex: number,
  tickSpacing: number
): AccountMeta[] {
  return [
    {
      pubkey: getTickArrayAddressByTick(
        programId,
        poolState,
        tickIndex,
        tickSpacing
      ),
      isWritable: true,
      isSigner: false,
    },
  ];
}

/**
 * Compute a valid tick for opening a limit order.
 *
 * Limit order PDA seeds: [owner, limit_order_nonce, order_nonce]
 *
 * @param tickCurrent - Pool's current tick
 * @param tickSpacing - Pool's tick spacing
 * @param zeroForOne - Order direction
 * @param tickOffset - Extra offset in tickSpacing units (default 0) for unique tick indices within a test
 */
export function getValidTickForLimitOrder(
  tickCurrent: number,
  tickSpacing: number,
  zeroForOne: boolean,
  tickOffset = 0
): number {
  let baseTick: number;
  if (zeroForOne) {
    // zero_for_one: limit order must be placed strictly above current tick
    baseTick =
      Math.floor((tickCurrent + tickSpacing) / tickSpacing) * tickSpacing;
    if (baseTick <= tickCurrent) baseTick += tickSpacing;
    // Offset moves further to the right (higher ticks)
    return baseTick + tickOffset * tickSpacing;
  } else {
    // one_for_zero: limit order must be placed strictly below current tick
    baseTick =
      Math.floor((tickCurrent - tickSpacing) / tickSpacing) * tickSpacing;
    if (baseTick >= tickCurrent) baseTick -= tickSpacing;
    // Offset moves further to the left (lower ticks) so that tick_index < tick_current always holds
    return baseTick - tickOffset * tickSpacing;
  }
}

/**
 * Clean up all existing limit orders for a given owner.
 * This function attempts to fetch all limit order accounts and close them.
 * Should be called at the beginning of test suites to avoid PDA collisions.
 */
export async function cleanupAllLimitOrders(
  program: Program<any>,
  instructions: any,
  owner: anchor.web3.Keypair
): Promise<void> {
  try {
    // Get all limit order accounts for this owner
    // owner field is at offset 40 (8 bytes discriminator + 32 bytes pool_id)
    // Note: memcmp bytes should be base58 encoded string in Anchor
    const limitOrders = await program.account.limitOrderState.all([
      {
        memcmp: {
          offset: 40, // Skip discriminator (8 bytes) + pool_id (32 bytes)
          bytes: owner.publicKey.toBase58(),
        },
      },
    ]);
    for (const limitOrderAccount of limitOrders) {
      try {
        const limitOrderData = limitOrderAccount.account;
        const limitOrderPDA = limitOrderAccount.publicKey;

        // Check if unfilled amount is zero (can be closed directly)
        const unfilledAmount = limitOrderData.totalAmount.sub(
          limitOrderData.filledAmount
        );

        if (unfilledAmount.eq(new anchor.BN(0))) {
          // Try to close the limit order
          try {
            await instructions.closeLimitOrder({
              owner: owner,
              limitOrder: limitOrderPDA,
            });
            // console.log(
            //   `Closed limit order at tick ${limitOrderData.tickIndex}, zeroForOne=${limitOrderData.zeroForOne}`
            // );
          } catch (closeErr: any) {
            // Ignore errors when closing (might already be closed or other issues)
            console.log(
              `Could not close limit order at tick ${limitOrderData.tickIndex}, zeroForOne=${limitOrderData.zeroForOne}:`,
              closeErr.message
            );
          }
        } else {
          // If not fully filled, try to decrease unfilled amount to zero and then close
          try {
            // Decrease unfilled amount to zero
            await instructions.decreaseLimitOrder({
              owner: owner,
              poolState: limitOrderData.poolId,
              limitOrder: limitOrderPDA,
              amount: unfilledAmount,
              amountMin: new anchor.BN(0),
            });
            // Now try to close
            await instructions.closeLimitOrder({
              owner: owner,
              limitOrder: limitOrderPDA,
            });
            // console.log(
            //   `Cleaned up and closed limit order at tick ${limitOrderData.tickIndex}, zeroForOne=${limitOrderData.zeroForOne}`
            // );
          } catch (cleanupErr: any) {
            // Ignore errors during cleanup
            console.log(
              `Could not cleanup limit order at tick ${limitOrderData.tickIndex}, zeroForOne=${limitOrderData.zeroForOne}:`,
              cleanupErr.message
            );
          }
        }
      } catch (err: any) {
        // Skip this limit order if there's an error
        console.log(`Error processing limit order:`, err.message);
      }
    }
  } catch (err: any) {
    // If we can't fetch limit orders (e.g., no accounts exist), that's fine
    console.log(`No limit orders found or error fetching:`, err.message);
  }
}


const poolInfoCache = new Map<string, Promise<any>>();

async function loadPoolCompute(connection: Connection, poolId: PublicKey) {
  const key = `${(connection as any)._rpcEndpoint}:${poolId.toBase58()}`;
  let cached = poolInfoCache.get(key);
  if (!cached) {
    cached = (async () => {
      const raydium = await Raydium.load({
        connection,
        owner: Keypair.generate(), // read-only; never signs
        cluster: "mainnet", // only selects the (unused) raydium HTTP API endpoint
        disableLoadToken: true,
        disableFeatureCheck: true,
      });
      const data = await raydium.clmm.getPoolInfoFromRpc(poolId.toBase58());
      return {
        computePoolInfo: (data as any).computePoolInfo,
        tickCache: (data as any).tickData[poolId.toBase58()],
      };
    })();
    poolInfoCache.set(key, cached);
  }
  return cached;
}

export async function buildSwapRemainingAccounts(
  connection: Connection,
  poolId: PublicKey,
  inputMint: PublicKey,
  amount: anchor.BN,
  isBaseInput = true,
  sqrtPriceLimitX64?: anchor.BN
): Promise<AccountMeta[]> {
  const { computePoolInfo, tickCache } = await loadPoolCompute(connection, poolId);
  const exBitmap = computePoolInfo.exBitmapInfo;
  const blockTimestamp = Math.floor(Date.now() / 1000);

  let remainingAccounts: PublicKey[];
  if (isBaseInput) {
    // exact-in: `amount` is the input amount of `inputMint`.
    ({ remainingAccounts } = PoolUtils.getOutputAmountAndRemainAccounts(
      computePoolInfo,
      exBitmap,
      tickCache,
      inputMint,
      amount,
      blockTimestamp,
      sqrtPriceLimitX64
    ));
  } else {
    // exact-out: `amount` is the desired output amount; the exact-out simulation
    // is keyed by the OUTPUT mint, which is the pool's other mint.
    const mintA = new PublicKey(computePoolInfo.mintA.address);
    const mintB = new PublicKey(computePoolInfo.mintB.address);
    const outputMint = inputMint.equals(mintA) ? mintB : mintA;
    ({ remainingAccounts } = PoolUtils.getInputAmountAndRemainAccounts(
      computePoolInfo,
      exBitmap,
      tickCache,
      outputMint,
      amount,
      blockTimestamp,
      sqrtPriceLimitX64
    ));
  }

  if (remainingAccounts.length === 0) {
    throw new Error(
      "Swap simulation returned no tick arrays (amount too small or pool empty)"
    );
  }

  return remainingAccounts.map((pubkey: PublicKey) => ({
    pubkey,
    isWritable: true,
    isSigner: false,
  }));
}

// ---------------------------------------------------------------------------
// Surfpool surfnet helpers (mainnet-fork cheatcodes + RPC readiness)
// ---------------------------------------------------------------------------

// Low-level JSON-RPC call against the surfnet endpoint (for non-standard
// surfnet_* cheatcode methods the typed client does not expose).
async function surfnetRpc(
  connection: Connection,
  method: string,
  params: any[]
): Promise<any> {
  const endpoint = (connection as any)._rpcEndpoint as string;
  const res = await fetch(endpoint, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const json = await res.json();
  if (json.error) {
    throw new Error(`${method} failed: ${JSON.stringify(json.error)}`);
  }
  return json.result;
}

/** Block until the surfnet RPC answers `getVersion` (validator is up). */
export async function waitForRpc(
  connection: Connection,
  timeoutMs = 60_000
): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      await connection.getVersion();
      return;
    } catch (_e) {
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  throw new Error(`Surfpool RPC not reachable within ${timeoutMs}ms`);
}

/**
 * Credit `owner`'s associated token account for `mint` with `amount` raw units.
 * Creates the ATA if it does not exist. `tokenProgram` must match the mint's
 * owning token program (SPL Token or Token-2022).
 */
export async function setTokenAccount(
  connection: Connection,
  owner: PublicKey,
  mint: PublicKey,
  amount: bigint,
  tokenProgram: PublicKey
): Promise<void> {
  await surfnetRpc(connection, "surfnet_setTokenAccount", [
    owner.toBase58(),
    mint.toBase58(),
    { amount: Number(amount) },
    tokenProgram.toBase58(),
  ]);
}

/** Fund a pubkey with SOL (lamports) for fees / rent. */
export async function airdrop(
  connection: Connection,
  pubkey: PublicKey,
  lamports: number
): Promise<void> {
  const sig = await connection.requestAirdrop(pubkey, lamports);
  await connection.confirmTransaction(sig, "confirmed");
}
