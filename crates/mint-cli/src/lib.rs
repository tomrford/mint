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

pub use mint_core::{build, error};

pub mod layout {
    pub use mint_core::layout::*;

    pub mod args {
        pub use crate::layout_args::*;
        pub use mint_core::build::BlockSelector;
    }
}

pub mod output {
    pub use mint_core::output::*;

    pub mod args {
        pub use crate::output_args::OutputArgs;
        pub use mint_core::output::OutputFormat;
    }
}
