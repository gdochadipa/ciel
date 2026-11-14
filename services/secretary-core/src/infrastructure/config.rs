//! Bounded configuration snapshots shared by offline setup and Core composition.
use serde::{
    de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{collections::BTreeSet, fmt, fs, io::Read, path::Path};

const MAX_BYTES: u64 = 1024 * 1024;

/// Sanitized configuration failures; never include paths, keys or source values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// The file could not be opened or read.
    Unavailable,
    /// The selected input is not a regular, non-symlink file.
    UnsafeFile,
    /// The input exceeds the one MiB configuration limit.
    TooLarge,
    /// JSON syntax is invalid or incomplete.
    InvalidJson,
    /// Duplicate keys or invalid typed fields were found.
    InvalidSchema,
    /// The platform cannot enforce the required file-opening policy.
    UnsupportedPlatform,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "configuration unavailable",
            Self::UnsafeFile => "unsafe configuration file",
            Self::TooLarge => "configuration too large",
            Self::InvalidJson => "invalid configuration JSON",
            Self::InvalidSchema => "invalid configuration schema",
            Self::UnsupportedPlatform => "unsupported configuration platform",
        })
    }
}

impl std::error::Error for ConfigError {}

/// Read one bounded regular file and decode JSON without duplicate object keys.
///
/// This checks the final file, not administrator ownership of its ancestors.
/// It supplies no runtime authority, manifest admission or credential validation.
///
/// # Examples
///
/// ```no_run
/// use secretary_core::{infrastructure::config::read_json, Registry};
/// let registry: Registry = read_json(std::path::Path::new("registry.json"))?;
/// # Ok::<(), secretary_core::infrastructure::config::ConfigError>(())
/// ```
///
/// # Errors
/// Returns a sanitized error for unsafe, unavailable, oversized or invalid input.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, ConfigError> {
    if cfg!(not(unix)) {
        return Err(ConfigError::UnsupportedPlatform);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| ConfigError::Unavailable)?;
    if !metadata.is_file() {
        return Err(ConfigError::UnsafeFile);
    }
    if metadata.len() > MAX_BYTES {
        return Err(ConfigError::TooLarge);
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| ConfigError::Unavailable)?;
    let opened = file.metadata().map_err(|_| ConfigError::Unavailable)?;
    if !opened.is_file() {
        return Err(ConfigError::UnsafeFile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.dev() != opened.dev() || metadata.ino() != opened.ino() {
            return Err(ConfigError::UnsafeFile);
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ConfigError::Unavailable)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(ConfigError::TooLarge);
    }
    decode(&bytes)
}

pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ConfigError> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(ConfigError::TooLarge);
    }
    // Validate the same bytes before typed decoding: maps and tagged enums may
    // otherwise discard duplicate keys. Keep the typed decoder's original types.
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    UniqueKeys::deserialize(&mut decoder).map_err(classify)?;
    decoder.end().map_err(classify)?;
    serde_json::from_slice(bytes).map_err(classify)
}

fn classify(error: serde_json::Error) -> ConfigError {
    match error.classify() {
        serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
            ConfigError::InvalidJson
        }
        _ => ConfigError::InvalidSchema,
    }
}

struct UniqueKeys;

impl<'de> Deserialize<'de> for UniqueKeys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(Self)
    }
}

impl<'de> Visitor<'de> for UniqueKeys {
    type Value = Self;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON with unique object keys")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self, E> {
        Ok(self)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self, E> {
        Ok(self)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self, E> {
        Ok(self)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self, E> {
        Ok(self)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self, E> {
        Ok(self)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self, E> {
        Ok(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self, A::Error> {
        while sequence.next_element::<Self>()?.is_some() {}
        Ok(self)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self, A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate configuration key"));
            }
            map.next_value::<Self>()?;
        }
        Ok(self)
    }
}
