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

pub mod args;
pub mod commands;
pub mod data;
pub mod data_args;
pub mod layout_args;
pub mod output_args;
pub mod visuals;
