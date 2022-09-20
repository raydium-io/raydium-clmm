use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("LOK")]
    LOK,
    #[msg("Not approved")]
    NotApproved,
    #[msg("invalid update amm config flag")]
    InvalidUpdateConfigFlag,
    #[msg("Account lack")]
    AccountLack,
    #[msg("Remove liquitity, collect fees owed and reward then you can close position account")]
    ClosePositionErr,

    #[msg("Minting amount should be greater than 0")]
    ZeroMintAmount,

    #[msg("Tick index of lower must be smaller than upper")]
    InvaildTickIndex,
    #[msg("The lower tick must be below the upper tick")]
    TickInvaildOrder,
    #[msg("The tick must be greater, or equal to the minimum tick(-221818)")]
    TickLowerOverflow,
    #[msg("The tick must be lesser than, or equal to the maximum tick(221818)")]
    TickUpperOverflow,
    #[msg("tick % tick_spacing must be zero")]
    TickAndSpacingNotMatch,
    #[msg("Invaild tick array account")]
    InvalidTickArray,

    #[msg("Square root price limit overflow")]
    SqrtPriceLimitOverflow,
    // second inequality must be < because the price can never reach the price at the max tick
    #[msg("sqrt_price_x64 out of range")]
    SqrtPriceX64,

    // Liquidity Sub
    #[msg("Liquidity sub delta L must be smaller than before")]
    LiquiditySubValueErr,
    // Liquidity Add
    #[msg("Liquidity add delta L must be greater, or equal to before")]
    LiquidityAddValueErr,
    #[msg("Invaild liquidity when update position")]
    InvaildLiquidity,
    #[msg("Both token amount must not be zero while supply liquidity")]
    ForbidBothZeroForSupplyLiquidity,

    /// swap errors
    // Non fungible position manager
    #[msg("Transaction too old")]
    TransactionTooOld,
    #[msg("Price slippage check")]
    PriceSlippageCheck,
    #[msg("Too little output received")]
    TooLittleOutputReceived,
    #[msg("Too much input paid")]
    TooMuchInputPaid,
    #[msg("Swap special amount can not be zero")]
    InvaildSwapAmountSpecified,
    #[msg("Input pool vault is invalid")]
    InvalidInputPoolVault,
    #[msg("Swap input or output amount is too small")]
    TooSmallInputOrOutputAmount,

    /// reward errors
    #[msg("Invalid reward index")]
    InvalidRewardIndex,
    #[msg("Invalid reward init param")]
    InvalidRewardInitParam,
    #[msg("Invalid collect reward desired amount")]
    InvalidRewardDesiredAmount,
    #[msg("Invalid collect reward input account number")]
    InvalidRewardInputAccountNumber,
    #[msg("Invalid reward period")]
    InvalidRewardPeriod,
    #[msg("Modification of emissiones is allowed within 72 hours from the end of the previous cycle")]
    NotApproveUpdateRewardEmissiones,
    #[msg("uninitialized reward info")]
    UnInitializedRewardInfo,
}
