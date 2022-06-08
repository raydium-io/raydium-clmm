use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("LOK")]
    LOK,
    #[msg("Minting amount should be greater than 0")]
    ZeroMintAmount,

    #[msg("The lower tick must be below the upper tick")]
    TickInvaildOrder,

    #[msg("The tick must be greater, or equal to the minimum tick(-221818)")]
    TickLowerOverflow,

    #[msg("The tick must be lesser than, or equal to the maximum tick(221818)")]
    TickUpperOverflow,

    #[msg("tick % tick_spacing must be zero")]
    TickAndSpacingNotMatch,

    #[msg("Square root price limit overflow")]
    SqrtPriceLimitOverflow,

    // second inequality must be < because the price can never reach the price at the max tick
    #[msg("sqrt_price_x32 out of range")]
    SqrtPriceX32,

    // Liquidity Sub
    #[msg("Liquidity sub delta L must be smaller than before")]
    LiquiditySubValueErr,

    // Liquidity Add
    #[msg("Liquidity add delta L must be greater, or equal to before")]
    LiquidityAddValueErr,

    // Non fungible position manager
    #[msg("Transaction too old")]
    TransactionTooOld,

    #[msg("Price slippage check")]
    PriceSlippageCheck,

    #[msg("Not approved")]
    NotApproved,

    #[msg("Too little output received")]
    TooLittleOutputReceived,

    #[msg("Too much input paid")]
    TooMuchInputPaid,

    #[msg("Swap special amount can not be zero")]
    InvaildSwapAmountSpecified,

    #[msg("Tick index of lower must be smaller than upper")]
    InvaildTickIndex,

    #[msg("Invaild liquidity when update position")]
    InvaildLiquidity,
}
