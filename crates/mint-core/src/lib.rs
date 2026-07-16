#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unwrap_used
    )
)]

pub mod build;
pub mod data;
pub mod error;
pub mod fingerprint;
pub mod header;
pub mod layout;
pub mod output;
