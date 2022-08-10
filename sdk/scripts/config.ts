import {
  Connection,
  ConfirmOptions,
  PublicKey,
  Keypair,
  Signer,
  ComputeBudgetProgram,
  TransactionInstruction,
  SystemProgram,
  TransactionSignature,
} from "@solana/web3.js";


export const url = "https://api.devnet.solana.com"

export const programId = new PublicKey(
  "devKfPVu9CaDvG47KG7bDKexFvAY37Tgp6rPHTruuqU"
);

export const admin = Keypair.fromSecretKey(
  new Uint8Array([
    14, 86, 200, 241, 238, 214, 121, 105, 124, 164, 10, 15, 29, 30, 254, 7, 150,
    79, 247, 251, 252, 32, 167, 84, 253, 14, 236, 200, 224, 115, 233, 183, 8,
    157, 68, 21, 135, 51, 193, 168, 32, 35, 95, 106, 176, 244, 52, 162, 191, 34,
    41, 150, 47, 223, 25, 191, 200, 150, 231, 200, 147, 107, 233, 13,
  ])
);

export function localWallet(): Keypair {
  return Keypair.fromSecretKey(
    new Uint8Array([
      12, 14, 221, 123, 106, 110, 90, 126, 26, 140, 181, 162, 148, 212, 32, 1,
      59, 85, 4, 75, 39, 92, 134, 194, 81, 99, 237, 93, 16, 209, 25, 93, 89, 83,
      9, 155, 52, 216, 158, 126, 151, 206, 205, 63, 159, 129, 183, 145, 213,
      243, 142, 90, 227, 81, 149, 67, 240, 245, 14, 175, 230, 215, 89, 253,
    ])
  );
}
