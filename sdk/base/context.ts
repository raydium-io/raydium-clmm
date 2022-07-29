import { Provider, Program, Idl } from "@project-serum/anchor";
import { PublicKey, Connection, ConfirmOptions, Signer,TransactionInstruction,TransactionSignature,Transaction } from "@solana/web3.js";
import { Wallet } from "@project-serum/anchor/dist/cjs/provider";
import AmmV3Idl from "../anchor/amm_v3.json";
import { AmmV3 } from "../anchor/amm_v3";

export class Context {
  readonly connection: Connection;
  readonly wallet: Wallet;
  readonly opts: ConfirmOptions;
  readonly program: Program<AmmV3>;
  readonly provider: Provider;

  public constructor(
    connection: Connection,
    wallet: Wallet,
    programId: PublicKey,
    opts: ConfirmOptions = Provider.defaultOptions()
  ) {
    const provider = new Provider(connection, wallet, opts);
    const program = new Program(AmmV3Idl as Idl, programId, provider);
    this.connection = provider.connection;
    this.wallet = provider.wallet;
    this.opts = opts;
    this.program = program as unknown as Program<AmmV3>;
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
