import { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { Program} from "@project-serum/anchor";
import { AmmV3 } from "../../anchor/amm_v3";


export type SetProtocolFeeRateAccounts = {
    owner: PublicKey;
    ammConfig: PublicKey;
    systemprogram: PublicKey;
  };
  
  export function setProtocolFeeRateInstruction(
    program: Program<AmmV3>,
    protocolFeeRate: number,
    accounts: SetProtocolFeeRateAccounts
  ): Promise<TransactionInstruction> {
    return program.methods
      .setProtocolFeeRate(protocolFeeRate)
      .accounts(accounts)
      .instruction();
  }
  