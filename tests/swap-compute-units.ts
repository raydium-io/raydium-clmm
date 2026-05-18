import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import { AccountMeta, PublicKey } from "@solana/web3.js";
import { TestSetup } from "./utils/setup";
import { InstructionHelper } from "./utils/instructions";
import {
  measureComputeUnits,
  getTickArrayRemainingAccounts,
} from "./utils/util";
import { SqrtPriceMath, TickUtils } from "@raydium-io/raydium-sdk-v2";
import { assert } from "chai";

// This script can only be executed by the admin. Make sure to set the admin key in lib.rs when running locally.
describe("compute_units_test", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
  const user = provider.wallet.payer;
  const instructions = new InstructionHelper(program);


  describe("swap compute units comparison", () => {
    let dynamicFeeConfig: PublicKey;

    before(async () => {
      const dynamicFeeConfigIndex = 0;
      const filterPeriod = 60; // 1 minute
      const decayPeriod = 3600; // 1 hour
      const reductionFactor = 5000; // 0.5
      const dynamicFeeControl = 1000; // 0.01
      const maxVolatilityAccumulator = 100_000;
      await instructions.createDynamicFeeConfig({
        payer: user,
        index: dynamicFeeConfigIndex,
        filterPeriod,
        decayPeriod,
        reductionFactor,
        dynamicFeeControl,
        maxVolatilityAccumulator,
      });
      [dynamicFeeConfig] = await instructions.pda.getDynamicFeeConfigPDA(
        dynamicFeeConfigIndex
      );
    });

    // Helper function to get tick arrays for swap path
    // Collects tick arrays for current tick and specified ticks, then sorts by swap direction
    const getTickArraysForSwap = (
      programId: PublicKey,
      poolState: PublicKey,
      tickCurrent: number,
      tickSpacing: number,
      zeroForOne: boolean,
      targetTicks: number[]
    ): AccountMeta[] => {
      // Collect unique tick arrays with their start indices
      const tickArrayMap = new Map<string, { address: PublicKey; startIndex: number }>();

      const addTickArray = (tick: number) => {
        const address = TickUtils.getTickArrayAddressByTick(
          programId,
          poolState,
          tick,
          tickSpacing
        );
        const startIndex = TickUtils.getTickArrayStartIndexByTick(tick, tickSpacing);
        const key = address.toString();
        if (!tickArrayMap.has(key)) {
          tickArrayMap.set(key, { address, startIndex });
        }
      };

      // Always add current tick
      addTickArray(tickCurrent);

      // Add all target ticks
      for (const tick of targetTicks) {
        addTickArray(tick);
      }

      // Convert to array and sort by start index based on swap direction
      const tickArrays = Array.from(tickArrayMap.values());
      if (zeroForOne) {
        // Price decreases: sort by start index descending (high to low)
        tickArrays.sort((a, b) => b.startIndex - a.startIndex);
      } else {
        // Price increases: sort by start index ascending (low to high)
        tickArrays.sort((a, b) => a.startIndex - b.startIndex);
      }

      return tickArrays.map(({ address }) => ({
        pubkey: address,
        isWritable: true,
        isSigner: false,
      }));
    };

    // it("compare compute units: dynamic fee enabled vs disabled", async () => {
    //   // Common swap parameters
    //   const swapAmount = new anchor.BN(1_000_000_000_000);
    //   const isBaseInput = true;

    //   // Common position parameters
    //   const tickLowerIndex = -100;
    //   const tickUpperIndex = 100;
    //   const liquidity = new anchor.BN(1_000_000_000_000);
    //   const amount0Max = new anchor.BN(100_000_000_000);
    //   const amount1Max = new anchor.BN(100_000_000_000);

    //   // Create pool without dynamic fee
    //   const setup = new TestSetup(program, user);
    //   const poolStateBase = await setup.createPool(0);
    //   const poolStateDataBase = await program.account.poolState.fetch(
    //     poolStateBase
    //   );
    //   const tickSpacing = poolStateDataBase.tickSpacing;

    //   // Add liquidity
    //   await instructions.openPosition({
    //     payer: user,
    //     poolState: poolStateBase,
    //     tickLowerIndex: tickLowerIndex,
    //     tickUpperIndex: tickUpperIndex,
    //     liquidity: liquidity,
    //     amount0Max: amount0Max,
    //     amount1Max: amount1Max,
    //     positionNftOwner: user.publicKey,
    //     tokenVault0Mint: poolStateDataBase.tokenMint0,
    //     tokenVault1Mint: poolStateDataBase.tokenMint1,
    //   });

    //   // Create pool with dynamic fee (uses shared dynamicFeeConfig from before())
    //   // We need to create new tokens for the dynamic fee pool to avoid conflicts
    //   // Create a new TestSetup instance to get new tokens
    //   const setupDyn = new TestSetup(program, user);
    //   const poolStateDyn = await setupDyn.createCustomizablePool({
    //     tick: 0,
    //     ammConfig: poolStateDataBase.ammConfig,
    //     collectFeeOn: { fromInput: {} },
    //     enableDynamicFee: true,
    //     dynamicFeeConfig: dynamicFeeConfig,
    //   });

    //   const poolStateDataDyn = await program.account.poolState.fetch(
    //     poolStateDyn
    //   );

    //   // Add liquidity to dynamic fee pool
    //   await instructions.openPosition({
    //     payer: user,
    //     poolState: poolStateDyn,
    //     tickLowerIndex: tickLowerIndex,
    //     tickUpperIndex: tickUpperIndex,
    //     liquidity: liquidity,
    //     amount0Max: amount0Max,
    //     amount1Max: amount1Max,
    //     positionNftOwner: user.publicKey,
    //     tokenVault0Mint: poolStateDataDyn.tokenMint0,
    //     tokenVault1Mint: poolStateDataDyn.tokenMint1,
    //   });

    //   // Derive swap direction zeroForOne from input/output vault mints:
    //   // zeroForOne = true  => swapping token0 -> token1 (price decreases)
    //   // zeroForOne = false => swapping token1 -> token0 (price increases)
    //   const inputVaultMintBase = poolStateDataBase.tokenMint0;
    //   const zeroForOne = inputVaultMintBase.equals(poolStateDataBase.tokenMint0);

    //   // Set sqrtPriceLimitX64 based on swap direction:
    //   // - If zeroForOne = true  (token0 -> token1), price decreases, use tick -100
    //   // - If zeroForOne = false (token1 -> token0), price increases, use tick 100
    //   const limitTick = zeroForOne ? -100 : 100;
    //   const sqrtPriceLimitX64 = new anchor.BN(
    //     SqrtPriceMath.getSqrtPriceX64FromTick(limitTick).toString()
    //   );

    //   // Get tick arrays for swap path (current tick, tick -100, and tick 100)
    //   const remainingTickArraysBase = getTickArraysForSwap(
    //     program.programId,
    //     poolStateBase,
    //     poolStateDataBase.tickCurrent,
    //     tickSpacing,
    //     zeroForOne,
    //     [-100, 100]
    //   );

    //   const remainingTickArraysDyn = getTickArraysForSwap(
    //     program.programId,
    //     poolStateDyn,
    //     poolStateDataDyn.tickCurrent,
    //     tickSpacing,
    //     zeroForOne,
    //     [-100, 100]
    //   );

    //   // Perform swap on base pool (no dynamic fee)
    //   console.log("\n=== Testing Base Pool (No Dynamic Fee) ===");
    //   const txBase = await instructions.swapV2(
    //     {
    //       owner: user,
    //       ammConfig: poolStateDataBase.ammConfig,
    //       poolState: poolStateBase,
    //       inputVaultMint: poolStateDataBase.tokenMint0,
    //       outputVaultMint: poolStateDataBase.tokenMint1,
    //       amount: swapAmount,
    //       otherAmountThreshold: new anchor.BN(0),
    //       sqrtPriceLimitX64: sqrtPriceLimitX64,
    //       isBaseInput: isBaseInput,
    //       remainingAccounts: remainingTickArraysBase as AccountMeta[],
    //     },
    //     { skipPreflight: true }
    //   );

    //   const computeUnitsBase = await measureComputeUnits(
    //     provider.connection,
    //     txBase
    //   );
    //   console.log(`Base pool compute units: ${computeUnitsBase}`);

    //   const poolStateDataBaseAfter = await program.account.poolState.fetch(poolStateBase);
    //   assert.strictEqual(poolStateDataBaseAfter.sqrtPriceX64.toString(), sqrtPriceLimitX64.toString());

    //   // Perform swap on dynamic fee pool
    //   console.log("\n=== Testing Dynamic Fee Pool ===");
    //   const txDyn = await instructions.swapV2(
    //     {
    //       owner: user,
    //       ammConfig: poolStateDataDyn.ammConfig,
    //       poolState: poolStateDyn,
    //       inputVaultMint: poolStateDataDyn.tokenMint0,
    //       outputVaultMint: poolStateDataDyn.tokenMint1,
    //       amount: swapAmount,
    //       otherAmountThreshold: new anchor.BN(0),
    //       sqrtPriceLimitX64: sqrtPriceLimitX64,
    //       isBaseInput: isBaseInput,
    //       remainingAccounts: remainingTickArraysDyn as AccountMeta[],
    //     },
    //     { skipPreflight: true }
    //   );

    //   const computeUnitsDyn = await measureComputeUnits(
    //     provider.connection,
    //     txDyn
    //   );
    //   console.log(`Dynamic fee pool compute units: ${computeUnitsDyn}`);
    //   const poolStateDataDynAfter = await program.account.poolState.fetch(poolStateDyn);
    //   assert.strictEqual(poolStateDataDynAfter.sqrtPriceX64.toString(), sqrtPriceLimitX64.toString());

    //   // Compare results
    //   if (computeUnitsBase && computeUnitsDyn) {
    //     const overhead = computeUnitsDyn - computeUnitsBase;
    //     const overheadPercent = ((overhead / computeUnitsBase) * 100).toFixed(2);
    //     console.log(`\n=== Results ===`);
    //     console.log(`Overhead: ${overhead} compute units (${overheadPercent}%)`);
    //     console.log(
    //       `Base: ${computeUnitsBase.toLocaleString()} CU | Dynamic: ${computeUnitsDyn.toLocaleString()} CU`
    //     );
    //   } else {
    //     console.warn("Could not measure compute units. Transaction may have failed.");
    //   }
    // });

    it("compare compute units: dynamic-fee pool without vs with limit order fill", async () => {
      // Shared params for both pools
      const tickLowerIndex = -100;
      const tickUpperIndex = 100;
      const liquidity = new anchor.BN(1_000_000_000_000);
      const amount0Max = new anchor.BN(100_000_000_000);
      const amount1Max = new anchor.BN(100_000_000_000);
      const swapAmount = new anchor.BN(1_000_000_000_000);
      const isBaseInput = true;

      // --- Pool A: dynamic-fee, no limit order ---
      const setupNoLO = new TestSetup(program, user);
      const poolStateNoLO = await setupNoLO.createCustomizablePool({
        tick: 0,
        collectFeeOn: { fromInput: {} },
        enableDynamicFee: true,
        dynamicFeeConfig,
      });
      const poolStateDataNoLO = await program.account.poolState.fetch(
        poolStateNoLO
      );
      const tickSpacing = poolStateDataNoLO.tickSpacing;

      await instructions.openPosition({
        payer: user,
        poolState: poolStateNoLO,
        tickLowerIndex,
        tickUpperIndex,
        liquidity,
        amount0Max,
        amount1Max,
        positionNftOwner: user.publicKey,
        tokenVault0Mint: poolStateDataNoLO.tokenMint0,
        tokenVault1Mint: poolStateDataNoLO.tokenMint1,
      });

      const zeroForOne = true; // token0 -> token1, price decreases
      const limitTick = zeroForOne ? -100 : 100;
      const sqrtPriceLimitX64 = new anchor.BN(
        SqrtPriceMath.getSqrtPriceX64FromTick(limitTick).toString()
      );

      const remainingNoLO = getTickArraysForSwap(
        program.programId,
        poolStateNoLO,
        poolStateDataNoLO.tickCurrent,
        tickSpacing,
        zeroForOne,
        [-100, 100]
      );

      console.log("\n=== Dynamic-fee pool, no limit order ===");
      const txNoLO = await instructions.swapV2(
        {
          owner: user,
          ammConfig: poolStateDataNoLO.ammConfig,
          poolState: poolStateNoLO,
          inputVaultMint: poolStateDataNoLO.tokenMint0,
          outputVaultMint: poolStateDataNoLO.tokenMint1,
          amount: swapAmount,
          otherAmountThreshold: new anchor.BN(0),
          sqrtPriceLimitX64,
          isBaseInput,
          remainingAccounts: remainingNoLO as AccountMeta[],
        },
        { skipPreflight: true }
      );
      const computeUnitsNoLO = await measureComputeUnits(
        provider.connection,
        txNoLO
      );
      console.log(`No limit order compute units: ${computeUnitsNoLO?.toLocaleString()}`);
      const poolStateDataNoLOAfter = await program.account.poolState.fetch(poolStateNoLO);
      assert.strictEqual(poolStateDataNoLOAfter.sqrtPriceX64.toString(), sqrtPriceLimitX64.toString());

      // --- Pool B: dynamic-fee, with limit order (swap will fill it) ---
      const setupWithLO = new TestSetup(program, user);
      const poolStateWithLO = await setupWithLO.createCustomizablePool({
        tick: 0,
        collectFeeOn: { fromInput: {} },
        enableDynamicFee: true,
        dynamicFeeConfig,
      });
      const poolStateDataWithLO = await program.account.poolState.fetch(
        poolStateWithLO
      );

      await instructions.openPosition({
        payer: user,
        poolState: poolStateWithLO,
        tickLowerIndex,
        tickUpperIndex,
        liquidity,
        amount0Max,
        amount1Max,
        positionNftOwner: user.publicKey,
        tokenVault0Mint: poolStateDataWithLO.tokenMint0,
        tokenVault1Mint: poolStateDataWithLO.tokenMint1,
      });

      // Limit order in opposite direction so the swap fills it
      await instructions.openLimitOrder({
        owner: user,
        poolState: poolStateWithLO,
        tickIndex: -50,
        zeroForOne: !zeroForOne,
        amount: new anchor.BN(1_000_000),
      });

      const remainingWithLO = getTickArraysForSwap(
        program.programId,
        poolStateWithLO,
        poolStateDataWithLO.tickCurrent,
        poolStateDataWithLO.tickSpacing,
        zeroForOne,
        [-100, 100]
      );

      console.log("\n=== Dynamic-fee pool, swap fills limit order ===");
      const txWithLO = await instructions.swapV2(
        {
          owner: user,
          ammConfig: poolStateDataWithLO.ammConfig,
          poolState: poolStateWithLO,
          inputVaultMint: poolStateDataWithLO.tokenMint0,
          outputVaultMint: poolStateDataWithLO.tokenMint1,
          amount: swapAmount,
          otherAmountThreshold: new anchor.BN(0),
          sqrtPriceLimitX64,
          isBaseInput,
          remainingAccounts: remainingWithLO as AccountMeta[],
        },
        { skipPreflight: true }
      );
      const computeUnitsWithLO = await measureComputeUnits(
        provider.connection,
        txWithLO
      );
      console.log(`With limit order fill compute units: ${computeUnitsWithLO?.toLocaleString()}`);
      const poolStateDataWithLOAfter = await program.account.poolState.fetch(poolStateWithLO);
      assert.strictEqual(poolStateDataWithLOAfter.sqrtPriceX64.toString(), sqrtPriceLimitX64.toString());

      // Compare
      if (computeUnitsNoLO != null && computeUnitsWithLO != null) {
        const overhead = computeUnitsWithLO - computeUnitsNoLO;
        const overheadPercent = (
          (overhead / computeUnitsNoLO) *
          100
        ).toFixed(2);
        console.log("\n=== Results (dynamic-fee: no LO vs with LO fill) ===");
        console.log(
          `Overhead: ${overhead} CU (${overheadPercent}%)`
        );
        console.log(
          `No LO: ${computeUnitsNoLO.toLocaleString()} CU | With LO fill: ${computeUnitsWithLO.toLocaleString()} CU`
        );
      }
    });

  });
});
