use crate::errors::CoinflipError;
use crate::state::{RAYDIUM_CLMM, RAYDIUM_CLMM_DEVNET, RAYDIUM_CLMM_DEVNET_LEGACY};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
};

/// Frontend-built CLMM hop: 12 fixed SwapV2 accounts after payer, then tick arrays.
/// 0 amm_config, 1 pool_state, 2 input_ata, 3 output_ata, 4 input_vault, 5 output_vault,
/// 6 observation, 7 token_program, 8 token_2022, 9 memo, 10 input_mint, 11 output_mint, 12+ ticks
pub const HOP_FIXED: usize = 12;
pub const SWAP_V2_DISCRIMINATOR: [u8; 8] = [43, 4, 237, 11, 26, 201, 30, 98];

pub fn is_clmm_program(program: &Pubkey) -> bool {
    *program == RAYDIUM_CLMM
        || *program == RAYDIUM_CLMM_DEVNET
        || *program == RAYDIUM_CLMM_DEVNET_LEGACY
}

pub fn split_hops<'a, 'info>(
    remaining: &'a [AccountInfo<'info>],
    hop1_len: u8,
) -> Result<(&'a [AccountInfo<'info>], &'a [AccountInfo<'info>])> {
    if hop1_len == 0 {
        require!(remaining.len() >= HOP_FIXED, CoinflipError::InvalidSwapAccounts);
        return Ok((remaining, &[]));
    }
    let n = hop1_len as usize;
    require!(n >= HOP_FIXED && remaining.len() > n, CoinflipError::InvalidSwapAccounts);
    require!(remaining.len() - n >= HOP_FIXED, CoinflipError::InvalidSwapAccounts);
    Ok((&remaining[..n], &remaining[n..]))
}

pub fn swap_exact_out<'info>(
    clmm: AccountInfo<'info>,
    owner: AccountInfo<'info>,
    hop: &[AccountInfo<'info>],
    expected_pool: &Pubkey,
    max_amount_in: u64,
    amount_out: u64,
) -> Result<()> {
    require!(hop.len() >= HOP_FIXED, CoinflipError::InvalidSwapAccounts);
    require!(is_clmm_program(&clmm.key()), CoinflipError::InvalidSwapAccounts);
    require_keys_eq!(hop[1].key(), *expected_pool, CoinflipError::InvalidPool);
    require!(amount_out > 0, CoinflipError::InvalidAmount);
    require!(max_amount_in > 0, CoinflipError::InvalidAmount);

    let mut data = SWAP_V2_DISCRIMINATOR.to_vec();
    data.extend_from_slice(&amount_out.to_le_bytes());
    data.extend_from_slice(&max_amount_in.to_le_bytes());
    data.extend_from_slice(&0u128.to_le_bytes());
    data.push(0); // is_base_input = false → exact out

    let mut accounts = vec![AccountMeta::new_readonly(owner.key(), true)];
    let mut infos = vec![owner.clone(), clmm.clone()];
    for (i, acc) in hop.iter().enumerate() {
        let writable = i != 0 && i != 7 && i != 8 && i != 9 && i != 10 && i != 11;
        accounts.push(if writable {
            AccountMeta::new(acc.key(), false)
        } else {
            AccountMeta::new_readonly(acc.key(), false)
        });
        infos.push(acc.clone());
    }

    invoke(
        &Instruction {
            program_id: clmm.key(),
            accounts,
            data,
        },
        &infos,
    )?;
    Ok(())
}

pub fn token_amount(account: &AccountInfo) -> Result<u64> {
    let data = account.try_borrow_data()?;
    require!(data.len() >= 72, CoinflipError::InvalidSwapAccounts);
    Ok(u64::from_le_bytes(data[64..72].try_into().unwrap()))
}
