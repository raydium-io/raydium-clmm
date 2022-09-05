import { Program } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
import { AmmV3 } from "../anchor/amm_v3";
import {
  PoolState,
  ProtocolPositionState,
  ObservationState,
  AmmConfig,
  TickArrayState,
  PersonalPositionState,
} from "./states";

export class StateFetcher {
  private program: Program<AmmV3>;

  constructor(program: Program<AmmV3>) {
    this.program = program;
  }

  public async getAmmConfig(address: PublicKey): Promise<AmmConfig> {
    return (await this.program.account.ammConfig.fetch(address)) as AmmConfig;
  }

  public async getPoolState(address: PublicKey): Promise<PoolState> {
    return await this.program.account.poolState.fetch(address) as PoolState;
  }

  public async getMultiplePoolStates(
    addresses: PublicKey[]
  ): Promise<PoolState[]> {
    const result = await this.program.account.poolState.fetchMultiple(
      addresses
    );
    return result as PoolState[];
  }

  public async getTickArrayState(address: PublicKey): Promise<TickArrayState> {
    return (await this.program.account.tickArrayState.fetch(
      address
    )) as TickArrayState;
  }

  public async getMultipleTickArrayState(
    addresses: PublicKey[]
  ): Promise<TickArrayState[]> {
    const result = await this.program.account.tickArrayState.fetchMultiple(
      addresses
    );
    return result as TickArrayState[];
  }

  public async getPersonalPositionState(
    address: PublicKey
  ): Promise<PersonalPositionState> {
    return (await this.program.account.personalPositionState.fetch(
      address
    )) as PersonalPositionState;
  }

  public async getMultiplePersonalPositionStates(
    addresses: PublicKey[]
  ): Promise<PersonalPositionState[]> {
    const result =
      await this.program.account.personalPositionState.fetchMultiple(addresses);
    return result as PersonalPositionState[];
  }

  public async getProtocolPositionState(
    address: PublicKey
  ): Promise<ProtocolPositionState> {
    return (await this.program.account.protocolPositionState.fetch(
      address
    )) as ProtocolPositionState;
  }

  public async getObservationState(
    address: PublicKey
  ): Promise<ObservationState> {
    return (await this.program.account.observationState.fetch(
      address
    )) as ObservationState;
  }
}
