import {
  Connection,
  SendOptions,
  Signer,
  Transaction,
  TransactionInstruction,
  TransactionSignature,
  ConfirmOptions,
  ComputeBudgetProgram,
} from "@solana/web3.js";

export async function sendTransaction(
  connection: Connection,
  ixs: TransactionInstruction[],
  signers: Array<Signer>,
  options?: ConfirmOptions,
): Promise<TransactionSignature> {
  const tx = new Transaction();
  for (var i = 0; i < ixs.length; i++) {
    tx.add(ixs[i]);
  }

  if (options == undefined) {
    options = {
      preflightCommitment: "confirmed",
      commitment: "confirmed",
    };
  }

  const sendOpt = options && {
    skipPreflight: options.skipPreflight,
    preflightCommitment: options.preflightCommitment || options.commitment,
  };

  tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  const signature = await connection.sendTransaction(tx, signers, sendOpt);

  const status = (
    await connection.confirmTransaction(signature, options.commitment)
  ).value;

  if (status.err) {
    throw new Error(
      `Raw transaction ${signature} failed (${JSON.stringify(status)})`
    );
  }
  return signature;
}
