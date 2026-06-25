use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Off,
    Lite,
    Full,
    Ultra,
}

pub const DEFAULT_MODE: Mode = Mode::Full;
pub const ALL_MODES: &[Mode] = &[Mode::Off, Mode::Lite, Mode::Full, Mode::Ultra];

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Lite => "lite",
            Mode::Full => "full",
            Mode::Ultra => "ultra",
        }
    }

    pub fn runs_transforms(self) -> bool {
        self != Mode::Off
    }

    pub fn offload_threshold(self) -> Option<f32> {
        match self {
            Mode::Off => None,
            Mode::Lite => Some(0.8),
            Mode::Full => Some(0.5),
            Mode::Ultra => Some(0.2),
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
#[error("unknown mode: {0}")]
pub struct ModeParseError(pub String);

impl FromStr for Mode {
    type Err = ModeParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Mode::Off),
            "lite" => Ok(Mode::Lite),
            "full" => Ok(Mode::Full),
            "ultra" => Ok(Mode::Ultra),
            other => Err(ModeParseError(other.to_string())),
        }
    }
}
