import * as anchor from "@project-serum/anchor";

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
