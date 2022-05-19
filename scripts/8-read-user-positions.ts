import fetch from 'node-fetch'
import * as anchor from '@project-serum/anchor'
import { BN, Program, web3 } from '@project-serum/anchor'
import { CyclosCore, FACTORY_ADDRESS, Pool, Position, POSITION_SEED, TICK_SEED, u32ToSeed } from '@cykura/sdk'
import { Token, TOKEN_PROGRAM_ID } from '@solana/spl-token'
import { Token as UniToken } from '@cykura/sdk-core'
import JSBI from 'jsbi'

const factoryState = 'DBsMwKfeoUHhxMi9x6wd2AsT12UwUCssjNbUzu1aKgqj'
const Q32 = new BN(2).shln(31)

export async function main() {
  const keypair = new web3.Keypair()
  const wallet = new anchor.Wallet(keypair)
  const connection = new web3.Connection('https://api.mainnet-beta.solana.com')
  const provider = new anchor.Provider(connection, wallet, {})
  anchor.setProvider(provider)
  const cyclosCore = anchor.workspace.CyclosCore as Program<CyclosCore>

  // get NFTs from indexer
  const response = await fetch('https://api.covalenthq.com/v1/1399811149/address/EwnJEKATSWtVVZX8sJbbcGPFMjXQw5paPYRvtd2GieoL/balances_v2/?key=ckey_888bd64636064665b65354aa956&nft=true')
  const nfts = (await response.json()).data.items

  for (const nft of nfts) {
    if (nft.nft_data?.[0].updateAuthority === factoryState) {
      const mint = new web3.PublicKey(nft.nft_data?.[0].mint)
      console.log('position NFT mint', nft.nft_data?.[0].mint)

      // derive tokenized position state
      const [tokenizedPositionState] = await web3.PublicKey.findProgramAddress([
        POSITION_SEED,
        mint.toBuffer()
      ], FACTORY_ADDRESS)

      const { poolId, liquidity, tickLower, tickUpper, tokensOwed0, tokensOwed1, feeGrowthInside0LastX32, feeGrowthInside1LastX32 } = await cyclosCore.account.tokenizedPositionState.fetch(tokenizedPositionState)

      // pool details for the position
      const { token0, token1, fee, sqrtPriceX32, tick, feeGrowthGlobal0X32, feeGrowthGlobal1X32 } = await cyclosCore.account.poolState.fetch(poolId)
      console.log(`Pool details: token0 ${token0}, token1 ${token1}, fee tier ${fee / 10000}%`)

      // Read token decimal places
      const token0Mint = new Token(
        connection,
        token0,
        TOKEN_PROGRAM_ID,
        keypair
      )
      const token1Mint = new Token(
        connection,
        token1,
        TOKEN_PROGRAM_ID,
        keypair
      )
      const { decimals: token0Decimals } = await token0Mint.getMintInfo()
      const { decimals: token1Decimals } = await token1Mint.getMintInfo()

      // derive liquidity composition of the pool, in terms of token0 and token1
      const pool = new Pool(
        new UniToken(101, token0, token0Decimals),
        new UniToken(101, token1, token1Decimals),
        fee,
        JSBI.BigInt(sqrtPriceX32),
        JSBI.BigInt(liquidity),
        tick
      )
      const position = new Position({
        pool,
        liquidity: JSBI.BigInt(liquidity),
        tickLower,
        tickUpper
      })
      console.log('Liquidity composition: token 0', position.amount0.toSignificant(), 'token 1', position.amount1.toSignificant())

      // Calculate unclaimed fees
      const [tickLowerState] = await anchor.web3.PublicKey.findProgramAddress(
        [TICK_SEED, token0.toBuffer(), token1.toBuffer(), u32ToSeed(fee), u32ToSeed(tickLower)],
        FACTORY_ADDRESS
      )
      const [tickUpperState] = await anchor.web3.PublicKey.findProgramAddress(
        [TICK_SEED, token0.toBuffer(), token1.toBuffer(), u32ToSeed(fee), u32ToSeed(tickUpper)],
        FACTORY_ADDRESS
      )

      const { feeGrowthOutside0X32: feeGrowthOutside0X32Lower, feeGrowthOutside1X32: feeGrowthOutside1X32Lower } =
        await cyclosCore.account.tickState.fetch(tickLowerState)
      const { feeGrowthOutside0X32: feeGrowthOutside0X32Upper, feeGrowthOutside1X32: feeGrowthOutside1X32Upper } =
        await cyclosCore.account.tickState.fetch(tickUpperState)

      const [feeGrowthBelow0X32, feeGrowthBelow1X32] =
        pool.tickCurrent >= tickLower
          ? [feeGrowthOutside0X32Lower, feeGrowthOutside1X32Lower]
          : [feeGrowthGlobal0X32.sub(feeGrowthOutside0X32Lower), feeGrowthGlobal1X32.sub(feeGrowthOutside1X32Lower)]

      const [feeGrowthAbove0X32, feeGrowthAbove1X32] =
        pool.tickCurrent < tickUpper
          ? [feeGrowthOutside0X32Upper, feeGrowthOutside1X32Upper]
          : [feeGrowthGlobal0X32.sub(feeGrowthOutside0X32Upper), feeGrowthGlobal1X32.sub(feeGrowthOutside1X32Upper)]

      const feeGrowthInside0X32 = feeGrowthGlobal0X32.sub(feeGrowthBelow0X32).sub(feeGrowthAbove0X32)
      const feeGrowthInside1X32 = feeGrowthGlobal1X32.sub(feeGrowthBelow1X32).sub(feeGrowthAbove1X32)

      const tokensOwed0Current = tokensOwed0.add(
        feeGrowthInside0X32.sub(feeGrowthInside0LastX32).mul(liquidity).div(Q32)
      )
      const tokensOwed1Current = tokensOwed1.add(
        feeGrowthInside1X32.sub(feeGrowthInside1LastX32).mul(liquidity).div(Q32)
      )

      // TODO divide by decimal places to get human readable numbers
      console.log('unclaimed fees: token0', tokensOwed0Current.toString(), 'token1', tokensOwed1Current.toString())

      console.log('----------------------------------------\n')
    }
  }
}

main()
