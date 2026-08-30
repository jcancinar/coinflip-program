use crate::errors::CoinflipError;
use crate::state::{Game, Side, RESULT_PREFIX};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    hash::hashv,
    instruction::{AccountMeta, Instruction},
    program::invoke,
};

pub const ORAO_VRF_ID: Pubkey = pubkey!("VRFzZoJdhFWL8rkvu87LpKM3RbcVezpMEc6X5GVDr7y");
pub const RANDOMNESS_ACCOUNT_SEED: &[u8] = b"orao-vrf-randomness-request";
pub const CONFIG_ACCOUNT_SEED: &[u8] = b"orao-vrf-network-configuration";
const REQUEST_V2_DISCRIMINATOR: [u8; 8] = [38, 151, 209, 6, 195, 102, 28, 217];

pub fn vrf_seed(game: &Pubkey) -> [u8; 32] {
    game.to_bytes()
}

pub fn randomness_address(vrf_program: &Pubkey, seed: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(&[RANDOMNESS_ACCOUNT_SEED, seed], vrf_program).0
}

pub fn network_state_address(vrf_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[CONFIG_ACCOUNT_SEED], vrf_program).0
}

pub fn result_winner(game: &Game, randomness: &[u8; 64]) -> Pubkey {
    let result_hash = hashv(&[
        RESULT_PREFIX,
        &game.creator_entropy,
        &game.joiner_entropy,
        &randomness[..32],
    ]);
    let winning_side = Side::from_result_bit(result_hash.to_bytes()[0] & 1);
    if game.creator_side == winning_side {
        game.creator
    } else {
        game.joiner
    }
}

pub fn request_randomness<'info>(
    vrf_program: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    network_state: &AccountInfo<'info>,
    treasury: &AccountInfo<'info>,
    request: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    expected_vrf: &Pubkey,
    seed: [u8; 32],
) -> Result<()> {
    require_keys_eq!(*vrf_program.key, *expected_vrf, CoinflipError::InvalidVrfProgram);
    require_keys_eq!(
        *network_state.key,
        network_state_address(expected_vrf),
        CoinflipError::InvalidVrfAccounts
    );
    require_keys_eq!(
        *request.key,
        randomness_address(expected_vrf, &seed),
        CoinflipError::InvalidVrfAccounts
    );

    let mut data = REQUEST_V2_DISCRIMINATOR.to_vec();
    data.extend_from_slice(&seed);
    invoke(
        &Instruction {
            program_id: *expected_vrf,
            accounts: vec![
                AccountMeta::new(*payer.key, true),
                AccountMeta::new(*network_state.key, false),
                AccountMeta::new(*treasury.key, false),
                AccountMeta::new(*request.key, false),
                AccountMeta::new_readonly(*system_program.key, false),
            ],
            data,
        },
        &[
            payer.clone(),
            network_state.clone(),
            treasury.clone(),
            request.clone(),
            system_program.clone(),
            vrf_program.clone(),
        ],
    )?;
    Ok(())
}

/// RandomnessV2: 8-byte disc, then RequestAccount tag (1 = Fulfilled), client, seed, [u8;64].
pub fn fulfilled_randomness(request: &AccountInfo, vrf_program: &Pubkey, seed: &[u8; 32]) -> Result<[u8; 64]> {
    require_keys_eq!(*request.owner, *vrf_program, CoinflipError::VrfNotFulfilled);
    require_keys_eq!(
        *request.key,
        randomness_address(vrf_program, seed),
        CoinflipError::InvalidVrfAccounts
    );
    let data = request.try_borrow_data()?;
    require!(data.len() >= 73 + 64, CoinflipError::VrfNotFulfilled);
    require!(data[8] == 1, CoinflipError::VrfNotFulfilled);
    let stored_seed: [u8; 32] = data[41..73].try_into().unwrap();
    require!(stored_seed == *seed, CoinflipError::InvalidVrfAccounts);
    Ok(data[73..137].try_into().unwrap())
}

pub fn is_fulfilled(request: &AccountInfo, vrf_program: &Pubkey, seed: &[u8; 32]) -> bool {
    fulfilled_randomness(request, vrf_program, seed).is_ok()
}
