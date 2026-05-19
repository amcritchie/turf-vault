use anchor_lang::prelude::*;
use crate::state::{VaultState, Season};
use crate::errors::VaultError;

/// Admin creates a Season with a per-entry seed-award schedule.
/// The schedule is set once at creation and is immutable thereafter.
///
/// PDA seeds: [b"season", season_id.to_le_bytes()] (u32 LE, 4 bytes).
///
/// Auth: any 1-of-3 vault signer (same routine-op pattern as mint_entry_token).
/// Re-creating a season with the same `season_id` is rejected by Anchor's
/// `init` constraint (PDA already exists).
#[derive(Accounts)]
#[instruction(season_id: u32)]
pub struct CreateSeason<'info> {
    /// Vault signer (admin) — pays SOL rent for the Season PDA.
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.bump,
        constraint = vault_state.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        init,
        payer = admin,
        space = Season::LEN,
        seeds = [b"season", season_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub season: Account<'info, Season>,

    pub system_program: Program<'info, System>,
}

pub fn handle_create_season(
    ctx: Context<CreateSeason>,
    season_id: u32,
    name: [u8; 32],
    seed_schedule: [u64; 5],
    start_at: i64,
) -> Result<()> {
    let season = &mut ctx.accounts.season;
    season.season_id = season_id;
    season.name = name;
    season.seed_schedule = seed_schedule;
    season.start_at = start_at;
    season.created_at = Clock::get()?.unix_timestamp;
    season.bump = ctx.bumps.season;

    msg!(
        "Season created: id={}, schedule=[{}, {}, {}, {}, {}], start_at={}",
        season_id,
        seed_schedule[0],
        seed_schedule[1],
        seed_schedule[2],
        seed_schedule[3],
        seed_schedule[4],
        start_at
    );
    Ok(())
}
