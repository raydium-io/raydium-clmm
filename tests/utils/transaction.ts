import {
  Connection,
  SendOptions,
  Signer,
  Transaction,
  TransactionInstruction,
  TransactionSignature,
} from "@solana/web3.js";

export async function sendTransaction(
  connection: Connection,
  ixs: TransactionInstruction[],
  signers: Array<Signer>,
  options?: SendOptions
): Promise<TransactionSignature> {
  const tx = new Transaction();
  for (var i = 0; i < ixs.length; i++) {
    tx.add(ixs[i]);
  }
  let sendOpt: SendOptions = {
    preflightCommitment: "processed",
  };
  if (options) {
    sendOpt = options;
  }
  tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  return connection.sendTransaction(tx, signers, sendOpt);
}
