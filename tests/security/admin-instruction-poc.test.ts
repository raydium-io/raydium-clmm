import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { RaydiumClmm } from "../../target/types/raydium_clmm";
import { PublicKey, Keypair } from "@solana/web3.js";
import { assert } from "chai";
import { TestSetup } from "./utils/setup";
import { InstructionHelper } from "./utils/instructions";
import { PDAUtils } from "./utils/pda";

describe("security: admin privileged instruction PoC for #191", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.raydiumClmm as Program<RaydiumClmm>;
  const user = provider.wallet.payer;
  const setup = new TestSetup(program, user);
  const instructions = new InstructionHelper(program);
  const pda = new PDAUtils(program.programId);
  let poolState: PublicKey;

  before(async () => {
    await setup.initialize();
    await setup.createTokens();
    await setup.createAmmConfig(0);
    poolState = await setup.createPool(0);
  });

  it("admin can update pool status without pool-owner confirmation", async () => {
    const before = await program.account.poolState.fetch(poolState);
    assert(before.status !== 1, "pool should not start disabled");

    await program.methods
      .updatePoolStatus(new anchor.BN(1))
      .accounts({
        authority: user.publicKey,
        poolState,
      })
      .signers([user])
      .rpc();

    const after = await program.account.poolState.fetch(poolState);
    assert.equal(after.status, 1, "pool status should be updated by admin");
  });

  it("admin can transfer reward owner and pool owner without pool-owner confirmation", async () => {
    const attacker = Keypair.generate();
    const attackerPubkey = attacker.publicKey;

    const before = await program.account.poolState.fetch(poolState);
    assert.notEqual(before.owner.toString(), attackerPubkey.toString());

    await program.methods
      .transferRewardOwner(attackerPubkey)
      .accounts({
        authority: user.publicKey,
        poolState,
      })
      .signers([user])
      .rpc();

    const after = await program.account.poolState.fetch(poolState);
    assert.equal(after.owner.toString(), attackerPubkey.toString());
  });

  it("admin can update amm config owner/fee params without config-owner confirmation", async () => {
    const [ammConfig] = await pda.getAmmConfigPDA(0);
    const newOwner = Keypair.generate().publicKey;

    await program.methods
      .updateAmmConfig(new anchor.BN(3), new anchor.BN(0))
      .accounts({
        owner: user.publicKey,
        ammConfig,
      })
      .remainingAccounts([
        {
          pubkey: newOwner,
          isSigner: false,
          isWritable: false,
        },
      ])
      .signers([user])
      .rpc();

    const after = await program.account.ammConfig.fetch(ammConfig);
    assert.equal(after.owner.toString(), newOwner.toString());
  });
});
