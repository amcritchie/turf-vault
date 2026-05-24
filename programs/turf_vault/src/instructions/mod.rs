// One file per instruction. Each module exports a Context<'info> Accounts
// struct and a `handle_*` function. `lib.rs` re-exports thin wrappers under
// `#[program]` that delegate here.
//
// Instructions are grouped roughly by lifecycle:
//   Vault setup & governance — initialize, force_close_vault, update_signers
//   Pause control (v0.15.0) — pause, unpause
//   User accounts           — create_user_account, set_username
//   Funds in/out            — deposit, withdraw
//   Seasons                 — create_season
//   Contest lifecycle       — create_contest, settle_contest, close_contest
//   Entries                 — enter_contest{,_direct,_with_token,_direct_with_token}
//   Free entries            — mint_entry_token

pub mod initialize;
pub mod create_user_account;
pub mod deposit;
pub mod withdraw;
pub mod create_contest;
pub mod enter_contest;
pub mod enter_contest_direct;
pub mod settle_contest;
pub mod close_contest;
pub mod force_close_vault;
pub mod update_signers;
pub mod mint_entry_token;
pub mod enter_contest_with_token;
pub mod enter_contest_direct_with_token;
pub mod create_season;
pub mod set_username;
pub mod pause;
pub mod unpause;

pub use initialize::*;
pub use create_user_account::*;
pub use deposit::*;
pub use withdraw::*;
pub use create_contest::*;
pub use enter_contest::*;
pub use enter_contest_direct::*;
pub use settle_contest::*;
pub use close_contest::*;
pub use force_close_vault::*;
pub use update_signers::*;
pub use mint_entry_token::*;
pub use enter_contest_with_token::*;
pub use enter_contest_direct_with_token::*;
pub use create_season::*;
pub use set_username::*;
pub use pause::*;
pub use unpause::*;
