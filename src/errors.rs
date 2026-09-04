use anchor_lang::prelude::*;

#[error_code]
pub enum CoinflipError {
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Program is paused")]
    Paused,
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("Amount does not match the game stake")]
    AmountMismatch,
    #[msg("Game already has a joiner")]
    AlreadyJoined,
    #[msg("Game is not ready to resolve")]
    NotReady,
    #[msg("Entropy must not be all zeros")]
    EntropyAllZero,
    #[msg("Creator cannot join their own game")]
    CannotJoinOwnGame,
    #[msg("Cancel is only allowed while the game is open")]
    CancelNotAllowed,
    #[msg("Invalid side for this game")]
    InvalidSide,
    #[msg("Winner account does not match the result")]
    InvalidWinner,
    #[msg("Fee BPS must be at most 500 (5% of pot)")]
    InvalidFeeBps,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("Minimum amount must be greater than zero when the token is enabled")]
    InvalidMinAmount,
    #[msg("Token is not enabled")]
    TokenNotEnabled,
    #[msg("Amount is below the token minimum")]
    AmountBelowMinimum,
    #[msg("Amount is above the token maximum")]
    AmountAboveMaximum,
    #[msg("Maximum must be at least the minimum when the token is enabled")]
    InvalidMaxAmount,
    #[msg("Mint does not match the game or token config")]
    InvalidMint,
    #[msg("Token accounts are required for this mint")]
    TokenAccountRequired,
    #[msg("Token account does not match the expected vault or owner")]
    TokenAccountMismatch,
    #[msg("Pool address is required when the token is enabled")]
    PoolRequired,
    #[msg("Quote mint must be SOL or USDC")]
    InvalidQuoteMint,
    #[msg("Swap pool does not match the configured pool")]
    InvalidPool,
    #[msg("Swap remaining accounts are invalid")]
    InvalidSwapAccounts,
    #[msg("This pay mint cannot join this game")]
    InvalidPayMint,
    #[msg("SOL-USDC pool is not configured")]
    SolUsdcPoolNotSet,
    #[msg("Swap exceeds the fixed 1% slippage")]
    SlippageExceeded,
    #[msg("Pool does not have enough liquidity for this swap")]
    InsufficientLiquidity,
    #[msg("Join slot hash is not available yet")]
    SlotHashNotReady,
    #[msg("Authority must initiate cancel and wait before closing the game")]
    AuthorityCancelNotInitiated,
    #[msg("Authority cancel delay has not elapsed")]
    AuthorityCancelNotReady,
    #[msg("VRF program does not match config")]
    InvalidVrfProgram,
    #[msg("VRF accounts do not match the game seed")]
    InvalidVrfAccounts,
    #[msg("ORAO randomness is not fulfilled yet")]
    VrfNotFulfilled,
    #[msg("Only the house can join this game")]
    HouseOnly,
    #[msg("House-only games require the creator to pick Heads or Tails")]
    HouseOnlyRequiresSide,
}
