import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program ,BN} from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";


export type CreateAmmConfigAccounts = {
    owner: PublicKey;
    ammConfig: PublicKey;
    systemProgram: PublicKey;
  };
  
  export function createAmmConfigInstruction(
    program: Program<AmmV3>,
    index:number,
    tickSpacing:number,
    globalFeeRate:number,
    protocolFeeRate: number,
    accounts: CreateAmmConfigAccounts
  ): Promise<TransactionInstruction> {
    return program.methods
      .createAmmConfig(index,tickSpacing,globalFeeRate,protocolFeeRate)
      .accounts(accounts)
      .instruction();
  }
  