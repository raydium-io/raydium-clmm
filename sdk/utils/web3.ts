import * as anchor from "@project-serum/anchor";
import {
  Connection,
  Signer,
  Transaction,
  TransactionInstruction,
  TransactionSignature,
  ConfirmOptions,
} from "@solana/web3.js";

export async function accountExist(
  connection: anchor.web3.Connection,
  account: anchor.web3.PublicKey
) {
  const info = await connection.getAccountInfo(account);
  if (info == null || info.data.length == 0) {
    return false;
  }
  return true;
}

export async function sendTransaction(
  connection: Connection,
  ixs: TransactionInstruction[],
  signers: Array<Signer>,
  options?: ConfirmOptions
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

export async function getBlockTimestamp(
  connection: Connection
): Promise<number> {
  let slot = await connection.getSlot();
  return await connection.getBlockTime(slot);
}
