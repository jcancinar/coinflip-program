use crate::errors::CoinflipError;
use crate::state::{
    effective_mint, is_sol_mint, max_swap_in_after_fee, min_swap_out_before_fee, trade_fee, Config,
    TokenConfig, CONFIG_SEED, TOKEN_PROGRAM_ID, TOKEN_SEED, TRADE_FEE_BPS, WSOL_MINT,
};
use crate::swap::{split_hops, swap_exact_in, swap_exact_out, token_amount, HOP_FIXED};
use crate::token_utils::transfer_tokens;
use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{
    close_account, sync_native, CloseAccount, Mint, SyncNative, TokenAccount, TokenInterface,
};

#[derive(Accounts)]
pub struct BuyToken<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        seeds = [TOKEN_SEED, token_config.mint.as_ref()],
        bump = token_config.bump
    )]
    pub token_config: Account<'info, TokenConfig>,
    pub mint_account: Box<InterfaceAccount<'info, Mint>>,
    pub pay_mint_account: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut)]
    pub user_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub user_pay_token: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: fee recipient is always `config.authority`
    #[account(
        mut,
        address = config.authority @ CoinflipError::Unauthorized
    )]
    pub fee_recipient: UncheckedAccount<'info>,
    #[account(mut)]
    pub fee_recipient_pay_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub wsol_account: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: Raydium CLMM program
    pub raydium_program: UncheckedAccount<'info>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SellToken<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,
    #[account(
        seeds = [TOKEN_SEED, token_config.mint.as_ref()],
        bump = token_config.bump
    )]
    pub token_config: Account<'info, TokenConfig>,
    pub mint_account: Box<InterfaceAccount<'info, Mint>>,
    pub receive_mint_account: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut)]
    pub user_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub user_receive_token: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: fee recipient is always `config.authority`
    #[account(
        mut,
        address = config.authority @ CoinflipError::Unauthorized
    )]
    pub fee_recipient: UncheckedAccount<'info>,
    #[account(mut)]
    pub fee_recipient_receive_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub wsol_account: Option<Box<InterfaceAccount<'info, TokenAccount>>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: Raydium CLMM program
    pub raydium_program: UncheckedAccount<'info>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

/// Buy an exact amount of an enabled token, paying with SOL or USDC (exact-out).
/// `max_pay` is the max total spend **including** the 1% protocol fee.
pub fn buy_token<'info>(
    ctx: Context<'_, '_, 'info, 'info, BuyToken<'info>>,
    amount_out: u64,
    pay_mint: Pubkey,
    max_pay: u64,
    quote_out: u64,
    hop1_len: u8,
) -> Result<()> {
    require!(!ctx.accounts.config.paused, CoinflipError::Paused);
    require!(amount_out > 0 && max_pay > 0, CoinflipError::InvalidAmount);
    validate_enabled_trade(
        &ctx.accounts.token_config,
        &ctx.accounts.mint_account.key(),
        amount_out,
        true,
    )?;
    require_pay_or_receive_mint(&ctx.accounts.config, &pay_mint)?;
    require_keys_eq!(
        ctx.accounts.pay_mint_account.key(),
        effective_mint(&pay_mint),
        CoinflipError::InvalidMint
    );
    require_keys_eq!(
        ctx.accounts.user_token.mint,
        ctx.accounts.token_config.mint,
        CoinflipError::InvalidMint
    );
    require_keys_eq!(
        ctx.accounts.user_token.owner,
        ctx.accounts.user.key(),
        CoinflipError::TokenAccountMismatch
    );
    require_keys_eq!(
        ctx.accounts.user_pay_token.mint,
        effective_mint(&pay_mint),
        CoinflipError::InvalidMint
    );
    require_keys_eq!(
        ctx.accounts.user_pay_token.owner,
        ctx.accounts.user.key(),
        CoinflipError::TokenAccountMismatch
    );
    require_keys_eq!(
        ctx.accounts.fee_recipient_pay_token.mint,
        effective_mint(&pay_mint),
        CoinflipError::InvalidMint
    );
    require_keys_eq!(
        ctx.accounts.fee_recipient_pay_token.owner,
        ctx.accounts.fee_recipient.key(),
        CoinflipError::TokenAccountMismatch
    );

    let max_swap_in = max_swap_in_after_fee(max_pay)?;
    require!(max_swap_in > 0, CoinflipError::InvalidAmount);

    let (first_pool, second_pool) = trade_pools(
        &ctx.accounts.config,
        &ctx.accounts.token_config,
        &pay_mint,
        true,
    )?;

    if is_sol_mint(&pay_mint) {
        wrap_sol_buy(&ctx, max_pay)?;
    }

    let user_token_info = ctx.accounts.user_token.to_account_info();
    let pay_before = token_amount(&ctx.accounts.user_pay_token.to_account_info())?;
    let token_before = token_amount(&user_token_info)?;
    let clmm = ctx.accounts.raydium_program.to_account_info();
    let owner = ctx.accounts.user.to_account_info();
    let remaining = ctx.remaining_accounts;

    if let Some(second_pool) = second_pool {
        require!(quote_out > 0 && hop1_len > 0, CoinflipError::InvalidSwapAccounts);
        let (hop1, hop2) = split_hops(remaining, hop1_len)?;
        require_two_hop_atas(
            hop1,
            hop2,
            &ctx.accounts.user_pay_token.key(),
            &ctx.accounts.user_token.key(),
        )?;
        swap_exact_out(
            clmm.clone(),
            owner.clone(),
            hop1,
            &first_pool,
            max_swap_in,
            quote_out,
        )?;
        swap_exact_out(
            clmm,
            owner,
            hop2,
            &second_pool,
            token_amount(&hop2[2])?,
            amount_out,
        )?;
    } else {
        let (hop, rest) = split_hops(remaining, 0)?;
        require!(rest.is_empty(), CoinflipError::InvalidSwapAccounts);
        require_hop_atas(
            hop,
            &ctx.accounts.user_pay_token.key(),
            &ctx.accounts.user_token.key(),
        )?;
        swap_exact_out(clmm, owner, hop, &first_pool, max_swap_in, amount_out)?;
    }

    require!(
        balance_increase(&user_token_info, token_before)? >= amount_out,
        CoinflipError::SlippageExceeded
    );

    let pay_after = token_amount(&ctx.accounts.user_pay_token.to_account_info())?;
    let spent = pay_before
        .checked_sub(pay_after)
        .ok_or(CoinflipError::ArithmeticOverflow)?;
    let fee = trade_fee(spent, TRADE_FEE_BPS)?;
    if fee > 0 {
        // Pay mint is always WSOL/USDC (Tokenkeg). Do not use token_program —
        // clients pass Token-2022 for the xStock mint.
        transfer_tokens(
            tokenkeg_program_buy(&ctx)?,
            ctx.accounts.user_pay_token.to_account_info(),
            ctx.accounts.pay_mint_account.to_account_info(),
            ctx.accounts.fee_recipient_pay_token.to_account_info(),
            ctx.accounts.user.to_account_info(),
            fee,
            ctx.accounts.pay_mint_account.decimals,
        )?;
    }

    if is_sol_mint(&pay_mint) {
        unwrap_wsol_buy(&ctx)?;
    }
    Ok(())
}

/// Sell an exact amount of an enabled token for SOL or USDC (exact-in).
/// `min_out` is the minimum the user must receive **after** the 1% protocol fee.
pub fn sell_token<'info>(
    ctx: Context<'_, '_, 'info, 'info, SellToken<'info>>,
    amount_in: u64,
    receive_mint: Pubkey,
    min_out: u64,
    min_quote_out: u64,
    hop1_len: u8,
) -> Result<()> {
    require!(!ctx.accounts.config.paused, CoinflipError::Paused);
    require!(amount_in > 0 && min_out > 0, CoinflipError::InvalidAmount);
    validate_enabled_trade(
        &ctx.accounts.token_config,
        &ctx.accounts.mint_account.key(),
        amount_in,
        false,
    )?;
    require_pay_or_receive_mint(&ctx.accounts.config, &receive_mint)?;
    require_keys_eq!(
        ctx.accounts.receive_mint_account.key(),
        effective_mint(&receive_mint),
        CoinflipError::InvalidMint
    );
    require_keys_eq!(
        ctx.accounts.user_token.mint,
        ctx.accounts.token_config.mint,
        CoinflipError::InvalidMint
    );
    require_keys_eq!(
        ctx.accounts.user_token.owner,
        ctx.accounts.user.key(),
        CoinflipError::TokenAccountMismatch
    );
    require_keys_eq!(
        ctx.accounts.user_receive_token.mint,
        effective_mint(&receive_mint),
        CoinflipError::InvalidMint
    );
    require_keys_eq!(
        ctx.accounts.user_receive_token.owner,
        ctx.accounts.user.key(),
        CoinflipError::TokenAccountMismatch
    );
    require_keys_eq!(
        ctx.accounts.fee_recipient_receive_token.mint,
        effective_mint(&receive_mint),
        CoinflipError::InvalidMint
    );
    require_keys_eq!(
        ctx.accounts.fee_recipient_receive_token.owner,
        ctx.accounts.fee_recipient.key(),
        CoinflipError::TokenAccountMismatch
    );

    let min_swap_out = min_swap_out_before_fee(min_out)?;

    let (first_pool, second_pool) = trade_pools(
        &ctx.accounts.config,
        &ctx.accounts.token_config,
        &receive_mint,
        false,
    )?;

    let receive_before = token_amount(&ctx.accounts.user_receive_token.to_account_info())?;
    let clmm = ctx.accounts.raydium_program.to_account_info();
    let owner = ctx.accounts.user.to_account_info();
    let remaining = ctx.remaining_accounts;

    if let Some(second_pool) = second_pool {
        require!(hop1_len > 0, CoinflipError::InvalidSwapAccounts);
        let (hop1, hop2) = split_hops(remaining, hop1_len)?;
        require_two_hop_atas(
            hop1,
            hop2,
            &ctx.accounts.user_token.key(),
            &ctx.accounts.user_receive_token.key(),
        )?;
        let hop1_min = if min_quote_out > 0 { min_quote_out } else { 1 };
        let hop2_in_before = token_amount(&hop2[2])?;
        swap_exact_in(
            clmm.clone(),
            owner.clone(),
            hop1,
            &first_pool,
            amount_in,
            hop1_min,
        )?;
        let hop2_amount_in = balance_increase(&hop2[2], hop2_in_before)?;
        require!(hop2_amount_in > 0, CoinflipError::InvalidAmount);
        swap_exact_in(
            clmm,
            owner,
            hop2,
            &second_pool,
            hop2_amount_in,
            min_swap_out,
        )?;
    } else {
        let (hop, rest) = split_hops(remaining, 0)?;
        require!(rest.is_empty(), CoinflipError::InvalidSwapAccounts);
        require_hop_atas(
            hop,
            &ctx.accounts.user_token.key(),
            &ctx.accounts.user_receive_token.key(),
        )?;
        swap_exact_in(clmm, owner, hop, &first_pool, amount_in, min_swap_out)?;
    }

    let receive_after = token_amount(&ctx.accounts.user_receive_token.to_account_info())?;
    let received = receive_after
        .checked_sub(receive_before)
        .ok_or(CoinflipError::ArithmeticOverflow)?;
    let fee = trade_fee(received, TRADE_FEE_BPS)?;
    let net = received
        .checked_sub(fee)
        .ok_or(CoinflipError::ArithmeticOverflow)?;
    require!(net >= min_out, CoinflipError::SlippageExceeded);

    if fee > 0 {
        transfer_tokens(
            tokenkeg_program_sell(&ctx)?,
            ctx.accounts.user_receive_token.to_account_info(),
            ctx.accounts.receive_mint_account.to_account_info(),
            ctx.accounts.fee_recipient_receive_token.to_account_info(),
            ctx.accounts.user.to_account_info(),
            fee,
            ctx.accounts.receive_mint_account.decimals,
        )?;
    }

    if is_sol_mint(&receive_mint) {
        unwrap_wsol_sell(&ctx)?;
    }
    Ok(())
}

fn require_hop_atas(hop: &[AccountInfo], input: &Pubkey, output: &Pubkey) -> Result<()> {
    require!(hop.len() >= HOP_FIXED, CoinflipError::InvalidSwapAccounts);
    require_keys_eq!(hop[2].key(), *input, CoinflipError::TokenAccountMismatch);
    require_keys_eq!(hop[3].key(), *output, CoinflipError::TokenAccountMismatch);
    Ok(())
}

/// Edges must match the declared user ATAs; hop1 output must be hop2 input.
fn require_two_hop_atas(
    hop1: &[AccountInfo],
    hop2: &[AccountInfo],
    first_in: &Pubkey,
    last_out: &Pubkey,
) -> Result<()> {
    require!(
        hop1.len() >= HOP_FIXED && hop2.len() >= HOP_FIXED,
        CoinflipError::InvalidSwapAccounts
    );
    require_keys_eq!(hop1[2].key(), *first_in, CoinflipError::TokenAccountMismatch);
    require_keys_eq!(hop1[3].key(), hop2[2].key(), CoinflipError::TokenAccountMismatch);
    require_keys_eq!(hop2[3].key(), *last_out, CoinflipError::TokenAccountMismatch);
    Ok(())
}

fn balance_increase(account: &AccountInfo, before: u64) -> Result<u64> {
    token_amount(account)?
        .checked_sub(before)
        .ok_or(error!(CoinflipError::ArithmeticOverflow))
}

fn validate_enabled_trade(
    token_config: &TokenConfig,
    mint: &Pubkey,
    amount: u64,
    enforce_min: bool,
) -> Result<()> {
    require!(token_config.is_enabled, CoinflipError::TokenNotEnabled);
    require_keys_eq!(token_config.mint, *mint, CoinflipError::InvalidMint);
    require!(token_config.pool != Pubkey::default(), CoinflipError::PoolRequired);
    if enforce_min {
        require!(amount >= token_config.min_amount, CoinflipError::AmountBelowMinimum);
    }
    if token_config.max_amount > 0 {
        require!(amount <= token_config.max_amount, CoinflipError::AmountAboveMaximum);
    }
    Ok(())
}

fn require_pay_or_receive_mint(config: &Config, mint: &Pubkey) -> Result<()> {
    if is_sol_mint(mint) {
        return Ok(());
    }
    require!(
        config.usdc_mint != Pubkey::default() && *mint == config.usdc_mint,
        CoinflipError::InvalidPayMint
    );
    Ok(())
}

/// Returns (first_pool, optional second_pool) for buy (`to_token`) or sell (`from_token`).
fn trade_pools(
    config: &Config,
    token_config: &TokenConfig,
    counter_mint: &Pubkey,
    buy: bool,
) -> Result<(Pubkey, Option<Pubkey>)> {
    let quote = effective_mint(&token_config.quote_mint);
    let counter = effective_mint(counter_mint);
    require!(
        is_sol_mint(&quote) || quote == config.usdc_mint,
        CoinflipError::InvalidQuoteMint
    );

    if counter == quote {
        return Ok((token_config.pool, None));
    }

    require!(!token_config.cross_disabled, CoinflipError::InvalidPayMint);

    // Cross SOL ↔ USDC then token pool (or reverse for sells).
    let via_usdc = counter == config.usdc_mint && is_sol_mint(&quote);
    let via_sol = is_sol_mint(&counter) && quote == config.usdc_mint;
    require!(via_usdc || via_sol, CoinflipError::InvalidPayMint);
    require!(
        config.sol_usdc_pool != Pubkey::default(),
        CoinflipError::SolUsdcPoolNotSet
    );

    if buy {
        // pay counter → quote on sol_usdc, then quote → token
        Ok((config.sol_usdc_pool, Some(token_config.pool)))
    } else {
        // sell token → quote, then quote → counter on sol_usdc
        Ok((token_config.pool, Some(config.sol_usdc_pool)))
    }
}

fn wrap_sol_buy<'info>(
    ctx: &Context<'_, '_, 'info, 'info, BuyToken<'info>>,
    lamports: u64,
) -> Result<()> {
    let wsol = ctx
        .accounts
        .wsol_account
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    require_keys_eq!(wsol.mint, WSOL_MINT, CoinflipError::InvalidMint);
    require_keys_eq!(wsol.owner, ctx.accounts.user.key(), CoinflipError::TokenAccountMismatch);
    require_keys_eq!(
        wsol.key(),
        ctx.accounts.user_pay_token.key(),
        CoinflipError::TokenAccountMismatch
    );
    transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: wsol.to_account_info(),
            },
        ),
        lamports,
    )?;
    sync_native(CpiContext::new(
        tokenkeg_program_buy(ctx)?,
        SyncNative {
            account: wsol.to_account_info(),
        },
    ))
}

fn unwrap_wsol_buy<'info>(ctx: &Context<'_, '_, 'info, 'info, BuyToken<'info>>) -> Result<()> {
    let wsol = ctx
        .accounts
        .wsol_account
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    require_keys_eq!(wsol.mint, WSOL_MINT, CoinflipError::InvalidMint);
    close_account(CpiContext::new(
        tokenkeg_program_buy(ctx)?,
        CloseAccount {
            account: wsol.to_account_info(),
            destination: ctx.accounts.user.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    ))
}

fn unwrap_wsol_sell<'info>(ctx: &Context<'_, '_, 'info, 'info, SellToken<'info>>) -> Result<()> {
    let wsol = ctx
        .accounts
        .wsol_account
        .as_ref()
        .ok_or(CoinflipError::TokenAccountRequired)?;
    require_keys_eq!(wsol.mint, WSOL_MINT, CoinflipError::InvalidMint);
    require_keys_eq!(
        wsol.key(),
        ctx.accounts.user_receive_token.key(),
        CoinflipError::TokenAccountMismatch
    );
    close_account(CpiContext::new(
        tokenkeg_program_sell(ctx)?,
        CloseAccount {
            account: wsol.to_account_info(),
            destination: ctx.accounts.user.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    ))
}

fn tokenkeg_program_buy<'info>(
    ctx: &Context<'_, '_, 'info, 'info, BuyToken<'info>>,
) -> Result<AccountInfo<'info>> {
    if ctx.accounts.token_program.key() == TOKEN_PROGRAM_ID {
        return Ok(ctx.accounts.token_program.to_account_info());
    }
    ctx.remaining_accounts
        .iter()
        .find(|account| account.key() == TOKEN_PROGRAM_ID)
        .cloned()
        .ok_or(error!(CoinflipError::TokenAccountRequired))
}

fn tokenkeg_program_sell<'info>(
    ctx: &Context<'_, '_, 'info, 'info, SellToken<'info>>,
) -> Result<AccountInfo<'info>> {
    if ctx.accounts.token_program.key() == TOKEN_PROGRAM_ID {
        return Ok(ctx.accounts.token_program.to_account_info());
    }
    ctx.remaining_accounts
        .iter()
        .find(|account| account.key() == TOKEN_PROGRAM_ID)
        .cloned()
        .ok_or(error!(CoinflipError::TokenAccountRequired))
}
