#![deny(unsafe_op_in_unsafe_fn)]

mod args;
mod config;
mod key;
mod windows;

use {
    crate::{
        args::Args,
        config::Config,
        windows::run,
    },
    eyre::{
        eyre,
        Result,
    },
    tracing::{
        debug,
        instrument,
        Level,
    },
    tracing_subscriber::{
        fmt::fmt,
        EnvFilter,
    },
};

#[instrument]
fn main() -> Result<()> {
    let Option::Some(args) = Args::get()? else {
        return Result::Ok(());
    };

    fmt()
        .with_ansi(args.ansi())
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env()?,
        )
        .try_init()
        .map_err(|error| eyre!(error))?;

    debug!("{args:?}");
    let config = Config::read(args.config())?;
    debug!("{config:?}");
    run(config)
}
