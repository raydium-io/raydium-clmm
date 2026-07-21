import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../../target/types/raydium_clmm";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  createAccount,
  mintTo,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { TestSetup } from "./setup";
import { InstructionHelper } from "./instructions";
import { PDAUtils } from "./pda";

describe("security: admin privileged instruction PoC for #191", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = provider.wallet.payer;
  const setup = new TestSetup(program, provider.wallet.payer);
  const instructions = new InstructionHelper(program);
  const pda = new PDAUtils(program.programId);
  let poolState: PublicKey;

  before(async () => {
    await setup.initialize();
    await setup.createTokens();
    await setup.createAmmConfig({
      admin: provider.wallet.payer,
      index: 0,
      tickSpacing: 1,
      tradeFeeRate: 100,
      protocolFeeRate: 20,
      fundFeeRate: 20,
    });
    poolState = await setup.createPool(0);
  });

  it("admin can update pool status without pool-owner confirmation", async () => {
    const before = await program.account.poolState.fetch(poolState);
    const beforeStatus = before.status;

    const adminAuthority = provider.wallet.payer;
    await program.methods
      .updatePoolStatus(new anchor.BN(1))
      .accounts({
        authority: adminAuthority.publicKey,
        poolState,
      })
      .signers([adminAuthority])
      .rpc();

    const after = await program.account.poolState.fetch(poolState);
    expect(after.status).to.equal(1);
  });

  it("admin can transfer reward owner and pool owner without pool-owner confirmation", async () => {
    const attacker = Keypair.generate();
    const attackerPubkey = attacker.publicKey;

    const before = await program.account.poolState.fetch(poolState);
    expect(before.owner).to.not.equal(attackerPubkey.toString());

    await program.methods
      .transferRewardOwner(attackerPubkey)
      .accounts({
        authority: provider.wallet.payer.publicKey,
        poolState,
      })
      .signers([provider.wallet.payer])
      .rpc();

    const after = await program.account.poolState.fetch(poolState);
    expect(after.owner.toString()).to.equal(attackerPubkey.toString());
  });

  it("admin can update amm config owner/fee params without config-owner confirmation", async () => {
    const [ammConfig] = await pda.getAmmConfigPDA(0);
    const before = await program.account.ammConfig.fetch(ammConfig);
    const newOwner = Keypair.generate().publicKey;

    await program.methods
      .updateAmmConfig(new anchor.BN(3), new anchor.BN(0))
      .accounts({
        owner: provider.wallet.payer.publicKey,
        ammConfig,
      })
      .remainingAccounts([
        {
          pubkey: newOwner,
          isSigner: false,
          isWritable: false,
        },
      ])
      .signers([provider.wallet.payer])
      .rpc();

    const after = await program.account.ammConfig.fetch(ammConfig);
    expect(after.owner.toString()).to.equal(newOwner.toString());
  });
});
