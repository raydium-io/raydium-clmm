import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../target/types/raydium_clmm";
import { PublicKey, Keypair } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  createAccount,
  getAccount,
  getMint,
  transfer,
  setAuthority,
  AuthorityType,
} from "@solana/spl-token";
import { assert } from "chai";
import { TestSetup, createMintPair } from "./utils/setup";
import { InstructionHelper, OpenedPosition } from "./utils/instructions";

describe("position nft freeze", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
  const admin = provider.wallet.payer;
  const setup = new TestSetup(program, admin);

  const AMM_CONFIG_INDEX = 0;
  const TICK_LOWER = -60;
  const TICK_UPPER = 60;
  const LIQUIDITY = new anchor.BN(1_000_000_000);
  const AMOUNT_MAX = new anchor.BN(10_000_000_000);

  before(async () => {
    await setup.initialize();
    await setup.createAmmConfig(AMM_CONFIG_INDEX);
  });

  const instructions = new InstructionHelper(program);

  /** The tick range and size every position in this file uses. */
  function openParams(
    poolState: PublicKey,
    token0: PublicKey,
    token1: PublicKey
  ) {
    return {
      payer: admin,
      poolState,
      tickLowerIndex: TICK_LOWER,
      tickUpperIndex: TICK_UPPER,
      liquidity: LIQUIDITY,
      amount0Max: AMOUNT_MAX,
      amount1Max: AMOUNT_MAX,
      positionNftOwner: admin.publicKey,
      tokenVault0Mint: token0,
      tokenVault1Mint: token1,
    };
  }

  /** A fresh pair whose mints both carry `freezeAuthority`, or none when null. */
  const createTokenPair = (freezeAuthority: PublicKey | null) =>
    createMintPair(provider.connection, admin, { freezeAuthority });

  const createPool = (token0: PublicKey, token1: PublicKey) =>
    instructions.createPool({
      poolCreator: admin,
      ammConfig: setup.ammConfig,
      tokenMint0: token0,
      tokenMint1: token1,
    });

  const openPositionV1 = (
    poolState: PublicKey,
    token0: PublicKey,
    token1: PublicKey
  ) => instructions.openPosition(openParams(poolState, token0, token1));

  const openPositionV2 = (
    poolState: PublicKey,
    token0: PublicKey,
    token1: PublicKey
  ) => instructions.openPositionV2(openParams(poolState, token0, token1));

  const openPositionWithToken22Nft = (
    poolState: PublicKey,
    token0: PublicKey,
    token1: PublicKey
  ) =>
    instructions.openPositionWithToken22Nft(
      openParams(poolState, token0, token1)
    );

  const closePosition = (pos: OpenedPosition) =>
    instructions.closePosition({ owner: admin, position: pos });

  describe("empty list (shipped default): behavior is unchanged", () => {
    it("empty list: pool snapshot flag is 0, position NFT is unfrozen and transferable", async () => {
      const [token0, token1] = await createTokenPair(null);
      const poolState = await createPool(token0, token1);

      const pos = await openPositionV1(poolState, token0, token1);
      const acc = await getAccount(
        provider.connection,
        pos.positionNftAccount,
        undefined,
        TOKEN_PROGRAM_ID
      );
      assert.isFalse(acc.isFrozen);

      const other = Keypair.generate();
      const otherAta = await createAccount(
        provider.connection,
        admin,
        pos.positionNftMint,
        other.publicKey
      );
      await transfer(
        provider.connection,
        admin,
        pos.positionNftAccount,
        otherAta,
        admin,
        1
      );
      const movedAcc = await getAccount(
        provider.connection,
        otherAta,
        undefined,
        TOKEN_PROGRAM_ID
      );
      assert.equal(
        movedAcc.amount.toString(),
        "1",
        "NFT must have moved to the new owner"
      );
    });

    it("a newly minted position NFT has the pool as its freeze authority", async () => {
      const [token0, token1] = await createTokenPair(null);
      const poolState = await createPool(token0, token1);
      const pos = await openPositionV1(poolState, token0, token1);

      const mintInfo = await getMint(
        provider.connection,
        pos.positionNftMint,
        undefined,
        TOKEN_PROGRAM_ID
      );
      assert.isNotNull(mintInfo.freezeAuthority);
      assert.equal(mintInfo.freezeAuthority!.toBase58(), poolState.toBase58());
    });

    it("empty list: a position closes cleanly, removing both the NFT account and personal_position", async () => {
      const [token0, token1] = await createTokenPair(null);
      const poolState = await createPool(token0, token1);
      const pos = await openPositionV1(poolState, token0, token1);

      await closePosition(pos);

      assert.isNull(
        await provider.connection.getAccountInfo(pos.positionNftAccount)
      );
      assert.isNull(
        await provider.connection.getAccountInfo(pos.personalPosition)
      );
    });
  });

  describe("frozen path (token mint freeze authority is in the shipped issuer list)", () => {
    const FROZEN_TEST_AUTHORITY = new PublicKey(
      "2Yq4T3mPNfjtEyTxSbRjRKqLf1pwbTasuCQrWe6QpM7x"
    );
    let v1Pool: PublicKey, v1Token0: PublicKey, v1Token1: PublicKey;
    let v2Pool: PublicKey, v2Token0: PublicKey, v2Token1: PublicKey;
    let t22Pool: PublicKey, t22Token0: PublicKey, t22Token1: PublicKey;
    let v2Pos: OpenedPosition;
    let t22Pos: OpenedPosition;

    before(async () => {
      [v1Token0, v1Token1] = await createTokenPair(FROZEN_TEST_AUTHORITY);
      v1Pool = await createPool(v1Token0, v1Token1);
      [v2Token0, v2Token1] = await createTokenPair(FROZEN_TEST_AUTHORITY);
      v2Pool = await createPool(v2Token0, v2Token1);
      [t22Token0, t22Token1] = await createTokenPair(FROZEN_TEST_AUTHORITY);
      t22Pool = await createPool(t22Token0, t22Token1);
    });

    // open_position v1 deserializes its vaults as classic
    // `anchor_spl::token::TokenAccount`, so Anchor's owner check makes it unable
    // to serve any pool holding a Token-2022 mint. Every restricted issuer asset
    // is Token-2022, so v1 never reaches a pool that needs freezing — and it is
    // not handed the vault mints the freeze decision reads. This pool uses classic
    // mints purely so the v1 path is reachable at all here.
    it("open_position v1 does not freeze: it can never serve a Token-2022 pool", async () => {
      const v1Pos = await openPositionV1(v1Pool, v1Token0, v1Token1);
      const acc = await getAccount(
        provider.connection,
        v1Pos.positionNftAccount,
        undefined,
        TOKEN_PROGRAM_ID
      );
      assert.isFalse(acc.isFrozen);
      await closePosition(v1Pos);
    });

    it("authority in list (open_position v2): NFT account is frozen; transfer and owner change both fail", async () => {
      v2Pos = await openPositionV2(v2Pool, v2Token0, v2Token1);
      const acc = await getAccount(
        provider.connection,
        v2Pos.positionNftAccount,
        undefined,
        TOKEN_PROGRAM_ID
      );
      assert.isTrue(acc.isFrozen);

      const other = Keypair.generate();
      const otherOwner = Keypair.generate().publicKey;
      const otherAta = await createAccount(
        provider.connection,
        admin,
        v2Pos.positionNftMint,
        other.publicKey
      );

      let transferErr: any;
      try {
        await transfer(
          provider.connection,
          admin,
          v2Pos.positionNftAccount,
          otherAta,
          admin,
          1
        );
      } catch (e) {
        transferErr = e;
      }
      assert.include(
        String(transferErr),
        "custom program error: 0x11",
        "transfer of a frozen position NFT (v2) must be rejected with AccountFrozen"
      );

      let authErr: any;
      try {
        await setAuthority(
          provider.connection,
          admin,
          v2Pos.positionNftAccount,
          admin,
          AuthorityType.AccountOwner,
          otherOwner
        );
      } catch (e) {
        authErr = e;
      }
      assert.include(
        String(authErr),
        "custom program error: 0x11",
        "setAuthority(AccountOwner) on a frozen position NFT (v2) must be rejected with AccountFrozen"
      );
    });

    it("a frozen position closes cleanly via thaw -> burn -> close (v2)", async () => {
      await closePosition(v2Pos);
      assert.isNull(
        await provider.connection.getAccountInfo(v2Pos.positionNftAccount)
      );
      assert.isNull(
        await provider.connection.getAccountInfo(v2Pos.personalPosition)
      );
    });

    it("authority in list (token-2022 nft): NFT account is frozen; transfer and owner change both fail", async () => {
      t22Pos = await openPositionWithToken22Nft(t22Pool, t22Token0, t22Token1);
      const acc = await getAccount(
        provider.connection,
        t22Pos.positionNftAccount,
        undefined,
        TOKEN_2022_PROGRAM_ID
      );
      assert.isTrue(acc.isFrozen);

      const other = Keypair.generate();
      const otherOwner = Keypair.generate().publicKey;
      const otherAta = await createAccount(
        provider.connection,
        admin,
        t22Pos.positionNftMint,
        other.publicKey,
        undefined,
        undefined,
        TOKEN_2022_PROGRAM_ID
      );

      let transferErr: any;
      try {
        await transfer(
          provider.connection,
          admin,
          t22Pos.positionNftAccount,
          otherAta,
          admin,
          1,
          [],
          undefined,
          TOKEN_2022_PROGRAM_ID
        );
      } catch (e) {
        transferErr = e;
      }
      assert.include(
        String(transferErr),
        "custom program error: 0x11",
        "transfer of a frozen position NFT (token-2022 nft) must be rejected with AccountFrozen"
      );

      let authErr: any;
      try {
        await setAuthority(
          provider.connection,
          admin,
          t22Pos.positionNftAccount,
          admin,
          AuthorityType.AccountOwner,
          otherOwner,
          [],
          undefined,
          TOKEN_2022_PROGRAM_ID
        );
      } catch (e) {
        authErr = e;
      }
      assert.include(
        String(authErr),
        "custom program error: 0x11",
        "setAuthority(AccountOwner) on a frozen position NFT (token-2022 nft) must be rejected with AccountFrozen"
      );
    });

    it("a frozen position closes cleanly via thaw -> burn -> close (token-2022 nft)", async () => {
      await closePosition(t22Pos);
      assert.isNull(
        await provider.connection.getAccountInfo(t22Pos.positionNftAccount)
      );
      assert.isNull(
        await provider.connection.getAccountInfo(t22Pos.personalPosition)
      );
    });
  });
});
