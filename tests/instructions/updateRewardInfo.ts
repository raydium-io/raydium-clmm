import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program ,BN} from "@project-serum/anchor";
import { AmmCore } from "../../target/types/amm_core";

export type UpdateRewardInfosAccounts = {
    ammConfig: PublicKey;
    poolState: PublicKey;
  };
  
  export function updateRewardInfos(
    program: Program<AmmCore>,
    accounts: UpdateRewardInfosAccounts
  ): Promise<TransactionInstruction> {
    return program.methods
      .updateRewardInfos()
      .accounts(accounts)
      .instruction();
  }
  