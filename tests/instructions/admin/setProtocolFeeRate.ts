import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program} from "@project-serum/anchor";
import { AmmCore } from "../../anchor/amm_core";


export type SetProtocolFeeRateAccounts = {
    owner: PublicKey;
    ammConfig: PublicKey;
    systemprogram: PublicKey;
  };
  
  export function setProtocolFeeRate(
    program: Program<AmmCore>,
    protocolFeeRate: number,
    accounts: SetProtocolFeeRateAccounts
  ): Promise<TransactionInstruction> {
    return program.methods
      .setProtocolFeeRate(protocolFeeRate)
      .accounts(accounts)
      .instruction();
  }
  