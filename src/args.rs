use {
    clap::{
        error::ErrorKind,
        Parser,
    },
    eyre::Result,
    std::path::{
        Path,
        PathBuf,
    },
};

#[derive(Debug, Parser)]
#[clap(version, about)]
pub struct Args {
    /// Enables ANSI terminal escape codes
    #[clap(short, long)]
    ansi: bool,
    /// Sets a configuration file
    #[clap(short, long, value_name = "FILE")]
    config: Option<PathBuf>,
}

impl Args {
    pub fn get() -> Result<Option<Self>> {
        match Self::try_parse() {
            Result::Ok(args) => Result::Ok(Option::Some(args)),
            Result::Err(error) => match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    println!("{error}");
                    Result::Ok(Option::None)
                },
                _ => Result::Err(error.into()),
            },
        }
    }

    pub fn ansi(&self) -> bool {
        self.ansi
    }

    pub fn config(&self) -> Option<&Path> {
        self.config.as_deref()
    }
}
