use {
    crate::key::Key,
    eyre::{
        OptionExt as _,
        Result,
    },
    rustc_hash::FxHashSet,
    serde::Deserialize,
    std::{
        borrow::Cow,
        env::var_os,
        fs::read_to_string,
        path::{
            Path,
            PathBuf,
        },
    },
    toml::from_str,
};

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "Config::default_exit")]
    pub exit: Key,
    #[serde(default)]
    pub fast: FxHashSet<Key>,
}

impl Config {
    pub fn read(path: Option<&Path>) -> Result<Config> {
        Result::Ok(from_str(&read_to_string(match path {
            Option::Some(path) => Cow::Borrowed(path),
            Option::None => {
                let mut path = PathBuf::from(var_os("APPDATA").ok_or_eyre("%APPDATA% is not set")?);
                path.push(concat!(env!("CARGO_PKG_NAME"), ".toml"));
                Cow::Owned(path)
            },
        })?)?)
    }

    fn default_exit() -> Key {
        Key::Escape
    }
}
