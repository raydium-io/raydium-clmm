/**
 * Swap test against a REAL mainnet CLMM pool, replayed on a local Surfpool
 * surfnet (mainnet fork).
 *
 * Program deployment is NOT handled here. The surfnet's deployment runbook
 * (runbooks/deployment/main.tx) resets the CLMM program's upgrade authority to
 * the local wallet and deploys our locally built `.so` over the canonical
 * mainnet address (CAMMCzo...) at startup. This test just drives the swap.
 *
 * What it does:
 *   1. Connects to a running surfnet (mainnet fork) RPC.
 *   2. Credits our own wallet with token0 + token1 balances (surfnet cheatcode,
 *      no mint authority required).
 *   3. Executes a swap_v2 on the live pool and asserts the output balance grew.
 *
 * Run it with a surfnet already running (`surfpool start` in another terminal):
 *   make test_live_swap
 *   # or: yarn ts-mocha -p ./tsconfig.json -t 1000000 tests/swap-live-pool.ts
 */
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  ComputeBudgetProgram,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  getAssociatedTokenAddressSync,
  getAccount,
} from "@solana/spl-token";
import { assert } from "chai";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import idl from "../target/idl/raydium_clmm.json";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import { PDAUtils } from "./utils/pda";
import {
  buildSwapRemainingAccounts,
  airdrop,
  setTokenAccount,
  waitForRpc,
} from "./utils/util";
import { MEMO_PROGRAM_ID, TickUtil } from "@raydium-io/raydium-sdk-v2";

// ---- Configurable parameters -------------------------------------------------
const RPC_URL = process.env.SURFPOOL_RPC || "http://127.0.0.1:8899";
const POOL_ID = new PublicKey(
  process.env.LIVE_POOL_ID || "CZkNKEwyeVJS2vHriYE3pG8CxRPL34PccTG88T6s5oAk"
);

function loadWallet(): Keypair {
  const walletPath =
    process.env.ANCHOR_WALLET ||
    process.env.SOLANA_WALLET ||
    path.join(os.homedir(), ".config", "solana", "id.json");
  const secret = JSON.parse(fs.readFileSync(walletPath, "utf-8"));
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

// Resolve which token program owns a mint (SPL Token vs Token-2022).
async function tokenProgramForMint(
  connection: Connection,
  mint: PublicKey
): Promise<PublicKey> {
  const info = await connection.getAccountInfo(mint);
  if (!info) throw new Error(`Mint ${mint.toBase58()} not found on the fork`);
  return info.owner.equals(TOKEN_2022_PROGRAM_ID)
    ? TOKEN_2022_PROGRAM_ID
    : TOKEN_PROGRAM_ID;
}

describe("swap on live mainnet pool (surfpool fork)", () => {
  const connection = new Connection(RPC_URL, "confirmed");
  const wallet = loadWallet();
  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(wallet),
    { commitment: "confirmed", skipPreflight: true }
  );
  anchor.setProvider(provider);

  const program = new Program(idl as any, provider) as Program<RaydiumClmm>;
  const pda = new PDAUtils(program.programId);

  let token0: PublicKey;
  let token1: PublicKey;
  let tokenProgram0: PublicKey;
  let tokenProgram1: PublicKey;
  let ammConfig: PublicKey;
  let observationState: PublicKey;
  let decimals0: number;

  before(async () => {
    await waitForRpc(connection);

    // SOL for fees (surfnet also airdrops to id.json at startup; this is a top-up).
    await airdrop(connection, wallet.publicKey, 100 * anchor.web3.LAMPORTS_PER_SOL);

    // Read the live pool (auto-fetched from mainnet by the fork).
    const pool = await program.account.poolState.fetch(POOL_ID);
    token0 = pool.tokenMint0 as PublicKey;
    token1 = pool.tokenMint1 as PublicKey;
    ammConfig = pool.ammConfig as PublicKey;
    observationState = pool.observationKey as PublicKey;
    decimals0 = pool.mintDecimals0 as number;

    tokenProgram0 = await tokenProgramForMint(connection, token0);
    tokenProgram1 = await tokenProgramForMint(connection, token1);

    console.log("pool:", POOL_ID.toBase58());
    console.log("token0:", token0.toBase58(), `(decimals ${decimals0})`);
    console.log("token1:", token1.toBase58());
    console.log("tickCurrent:", pool.tickCurrent, "tickSpacing:", pool.tickSpacing);
    console.log("liquidity:", pool.liquidity.toString());

    // Credit ourselves with both tokens. Fund a fixed number of WHOLE tokens of
    // each, scaled by that token's own decimals (so amounts are comparable
    // regardless of decimals, and stay < 2^53 for the cheatcode's JSON number
    // for tokens up to ~9 decimals).
    const wholeTokens = 1_000_000n; // 1,000,000 whole tokens of each
    const fund0 = wholeTokens * 10n ** BigInt(pool.mintDecimals0);
    const fund1 = wholeTokens * 10n ** BigInt(pool.mintDecimals1);
    await setTokenAccount(connection, wallet.publicKey, token0, fund0, tokenProgram0);
    await setTokenAccount(connection, wallet.publicKey, token1, fund1, tokenProgram1);
  });

  it("executes swap_v2 token0 <-> token1 on the live pool", async () => {
    const pool = await program.account.poolState.fetch(POOL_ID);

    // Swap direction for this case: token0 -> token1 (price decreasing).
    const zeroForOne = true;
    const inputMint = zeroForOne ? token0 : token1;
    const outputMint = zeroForOne ? token1 : token0;
    const inputTokenProgram = zeroForOne ? tokenProgram0 : tokenProgram1;
    const outputTokenProgram = zeroForOne ? tokenProgram1 : tokenProgram0;

    const [inputVault] = await pda.getTokenVaultPDA(POOL_ID, inputMint);
    const [outputVault] = await pda.getTokenVaultPDA(POOL_ID, outputMint);

    const inputAta = getAssociatedTokenAddressSync(
      inputMint,
      wallet.publicKey,
      false,
      inputTokenProgram
    );
    const outputAta = getAssociatedTokenAddressSync(
      outputMint,
      wallet.publicKey,
      false,
      outputTokenProgram
    );

    const amount = new anchor.BN(6).mul(
      new anchor.BN(10).pow(new anchor.BN(decimals0))
    );

    const remainingAccounts = await buildSwapRemainingAccounts(
      connection,
      POOL_ID,
      inputMint,
      amount
    );
    console.log(
      "remaining tick-array accounts:",
      remainingAccounts.map((m) => m.pubkey.toBase58())
    );

    const outBefore = BigInt(
      (await getAccount(connection, outputAta, "confirmed", outputTokenProgram)).amount
    );

    const sig = await program.methods
      .swapV2(
        amount,
        new anchor.BN(0), // otherAmountThreshold (min out)
        TickUtil.getSqrtPriceAtTick(-443635), // cap at far edge of provided tick arrays (partial fill if amount is huge)
        true // isBaseInput
      )
      .accounts({
        payer: wallet.publicKey,
        ammConfig,
        poolState: POOL_ID,
        inputTokenAccount: inputAta,
        outputTokenAccount: outputAta,
        inputVault,
        outputVault,
        observationState,
        inputVaultMint: inputMint,
        outputVaultMint: outputMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        tokenProgram2022: TOKEN_2022_PROGRAM_ID,
        memoProgram: MEMO_PROGRAM_ID,
      } as any)
      .remainingAccounts(remainingAccounts)
      .preInstructions([
        ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }),
      ])
      .signers([wallet])
      .rpc({ skipPreflight: true });

    console.log("swap tx:", sig);

    const outAfter = BigInt(
      (await getAccount(connection, outputAta, "confirmed", outputTokenProgram)).amount
    );
    const received = outAfter - outBefore;
    console.log("output received:", received.toString());

    assert.isTrue(received > 0n, "swap should produce a positive output amount");

    const poolAfter = await program.account.poolState.fetch(POOL_ID);
    assert.notStrictEqual(
      poolAfter.sqrtPriceX64.toString(),
      pool.sqrtPriceX64.toString(),
      "pool price should move after a swap"
    );
  });
});
