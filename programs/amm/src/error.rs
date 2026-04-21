use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Not approved")]
    NotApproved,
    #[msg("invalid update amm config flag")]
    InvalidUpdateConfigFlag,
    #[msg("Account lack")]
    AccountLack,
    #[msg("Remove liquidity, collect fees owed and reward then you can close position account")]
    ClosePositionErr,

    #[msg("Tick out of range")]
    InvalidTickIndex,
    #[msg("The lower tick must be below the upper tick")]
    TickInvalidOrder,
    #[msg("The tick must be greater, or equal to the minimum tick(-443636)")]
    TickLowerOverflow,
    #[msg("The tick must be lesser than, or equal to the maximum tick(443636)")]
    TickUpperOverflow,
    #[msg("tick % tick_spacing must be zero")]
    TickAndSpacingNotMatch,
    #[msg("Invalid tick array account")]
    InvalidTickArray,
    #[msg("Invalid tick array boundary")]
    InvalidTickArrayBoundary,

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
    #[msg("Both token amount must not be zero while supply liquidity")]
    ForbidBothZeroForSupplyLiquidity,
    #[msg("Liquidity insufficient")]
    LiquidityInsufficient,

    /// swap errors
    #[msg("Price slippage check")]
    PriceSlippageCheck,
    #[msg("Too little output received")]
    TooLittleOutputReceived,
    #[msg("Too much input paid")]
    TooMuchInputPaid,
    #[msg("Swap special amount can not be zero")]
    ZeroAmountSpecified,
    #[msg("Input pool vault is invalid")]
    InvalidInputPoolVault,
    #[msg("Swap input or output amount is too small")]
    TooSmallInputOrOutputAmount,
    #[msg("Not enough tick array account")]
    NotEnoughTickArrayAccount,
    #[msg("Invalid first tick array account")]
    InvalidFirstTickArrayAccount,

    /// reward errors
    #[msg("Invalid reward index")]
    InvalidRewardIndex,
    #[msg("The init reward token reach to the max")]
    FullRewardInfo,
    #[msg("The init reward token already in use")]
    RewardTokenAlreadyInUse,
    #[msg("The reward tokens must contain one of pool vault mint except the last reward")]
    ExceptRewardMint,
    #[msg("Invalid reward init param")]
    InvalidRewardInitParam,
    #[msg("Invalid collect reward input account number")]
    InvalidRewardInputAccountNumber,
    #[msg("Invalid reward period")]
    InvalidRewardPeriod,
    #[msg(
        "Modification of emissions is allowed within 72 hours from the end of the previous cycle"
    )]
    NotApproveUpdateRewardEmissions,
    #[msg("uninitialized reward info")]
    UnInitializedRewardInfo,

    #[msg("Not support token_2022 mint extension")]
    NotSupportMint,
    #[msg("Missing tickarray bitmap extension account")]
    MissingTickArrayBitmapExtensionAccount,
    #[msg("Insufficient liquidity for this direction")]
    InsufficientLiquidityForDirection,
    #[msg("Max token overflow")]
    MaxTokenOverflow,
    #[msg("Calculate overflow")]
    CalculateOverflow,
    #[msg("TransferFee calculate not match")]
    TransferFeeCalculateNotMatch,
    #[msg("Order already fully filled, cannot modify")]
    OrderAlreadyFilled,
    #[msg("Invalid order phase")]
    InvalidOrderPhase,
    #[msg("Invalid limit order amount")]
    InvalidLimitOrderAmount,
    #[msg("Tick order phase saturated")]
    OrderPhaseSaturated,

    #[msg("Invalid dynamic fee config params")]
    InvalidDynamicFeeConfigParams,
    #[msg("Invalid fee on which token (must be 0, 1, or 2)")]
    InvalidFeeOn,

    #[msg("sqrt_price_x64 must be greater than 0")]
    ZeroSqrtPrice,
    #[msg("liquidity must be greater than 0")]
    ZeroLiquidity,
    #[msg("base_flag is required when liquidity is zero")]
    MissingBaseFlag,
    #[msg("Mint account is required but not provided")]
    MissingMintAccount,
    #[msg("Token-2022 program is required but not provided")]
    MissingTokenProgram2022,
}
