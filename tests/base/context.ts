import { Provider, Program, Idl } from "@project-serum/anchor";
import { PublicKey, Connection, ConfirmOptions, Signer,TransactionInstruction,TransactionSignature,Transaction } from "@solana/web3.js";
import { Wallet } from "@project-serum/anchor/dist/cjs/provider";
import AmmCoreIdl from "../../target/idl/amm_core.json";
import { AmmCore } from "../../target/types/amm_core";

export class Context {
  readonly connection: Connection;
  readonly wallet: Wallet;
  readonly opts: ConfirmOptions;
  readonly program: Program<AmmCore>;
  readonly provider: Provider;

  public constructor(
    connection: Connection,
    wallet: Wallet,
    programId: PublicKey,
    opts: ConfirmOptions = Provider.defaultOptions()
  ) {
    const provider = new Provider(connection, wallet, opts);
    const program = new Program(AmmCoreIdl as Idl, programId, provider);
    this.connection = provider.connection;
    this.wallet = provider.wallet;
    this.opts = opts;
    this.program = program as unknown as Program<AmmCore>;
    this.provider = provider;
  }


  public async sendTransaction(
    ixs: TransactionInstruction[],
    signers?: Array<Signer | undefined>,
    opts?: ConfirmOptions
  ): Promise<TransactionSignature> {
    const tx = new Transaction();
    for (var i = 0; i < ixs.length; i++) {
      tx.add(ixs[i]);
    }
    return this.provider.send(tx, signers, opts);
  }

}
