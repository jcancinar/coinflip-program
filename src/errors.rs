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
    #[msg("Server entropy does not match the stored commitment")]
    BadReveal,
    #[msg("Entropy must not be all zeros")]
    EntropyAllZero,
    #[msg("Creator cannot join their own game")]
    CannotJoinOwnGame,
    #[msg("Cancel is only allowed while the game is open")]
    CancelNotAllowed,
    #[msg("Resolver commitment is missing")]
    CommitMissing,
    #[msg("Resolver commitment is already set")]
    CommitAlreadySet,
    #[msg("Invalid side for this game")]
    InvalidSide,
    #[msg("Winner account does not match the result")]
    InvalidWinner,
    #[msg("Fee BPS must be at most 10000")]
    InvalidFeeBps,
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
    #[msg("Minimum amount must be greater than zero when the token is enabled")]
    InvalidMinAmount,
    #[msg("Token is not enabled")]
    TokenNotEnabled,
    #[msg("Amount is below the token minimum")]
    AmountBelowMinimum,
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
}
