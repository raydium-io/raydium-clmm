use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("LOK")]
    LOK,
    #[msg("Minting amount should be greater than 0")]
    ZeroMintAmount,

    // states/pool.rs

    // The lower tick must be below the upper tick
    #[msg("TLU")]
    TLU,

    // The tick should be a multiple of tick spacing
    #[msg("TMS")]
    TMS,

    // The tick must be greater, or equal to, the minimum tick
    #[msg("TLM")]
    TLM,

    // The tick must be lesser than, or equal to, the maximum tick
    #[msg("TUM")]
    TUM,

    // Mint 0, The balance of token0 in the given pool before minting must be less than,
    // or equal to, the balance after minting
    #[msg("M0")]
    M0,

    // Mint 1, The balance of token1 in the given pool before minting must be less than,
    // or equal to, the balance after minting
    #[msg("M1")]
    M1,

    // Observation state seed should be valid
    #[msg("OS")]
    OS,

    // `amount_specified` cannot be zero
    #[msg("AS")]
    AS,

    // Square root price limit
    #[msg("SPL")]
    SPL,

    #[msg("IIA")]
    IIA,

    // states/position.rs

    // No poke/burn for a position with 0 liquidity
    #[msg("NP")]
    NP,

    // states/tick.rs

    // liquidity_gross_after must be less than max_liquidity
    #[msg("LO")]
    LO,

    // libraries/tick_math.rs

    // second inequality must be < because the price can never reach the price at the max tick
    #[msg("R")]
    R,
    // The given tick must be less than, or equal to, the maximum tick
    #[msg("T")]
    T,

    // libraries/liquidity_math.rs

    // Liquidity Sub
    #[msg("LS")]
    LS,

    // Liquidity Add
    #[msg("LA")]
    LA,

    // Non fungible position manager
    #[msg("Transaction too old")]
    TransactionTooOld,

    #[msg("Price slippage check")]
    PriceSlippageCheck,

    #[msg("Not approved")]
    NotApproved,

    // Swap router
    #[msg("Too little received")]
    TooLittleReceived,
}
