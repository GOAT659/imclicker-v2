use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindMode {
    Toggle,
    Hold,
}

impl Default for BindMode {
    fn default() -> Self {
        Self::Toggle
    }
}

impl BindMode {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Toggle => 0,
            Self::Hold => 1,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Hold,
            _ => Self::Toggle,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Toggle => "Toggle",
            Self::Hold => "Hold",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub target_cps: u32,
    pub mode: BindMode,
    pub bind_vk: u16,
    pub manual_active: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            target_cps: 120,
            mode: BindMode::Toggle,
            bind_vk: b'Y' as u16,
            manual_active: false,
        }
    }
}

fn config_dir() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("com", "openai", "imclicker_v2") {
        project_dirs.config_dir().to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();

    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<AppConfig>(&raw).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(config: &AppConfig) -> io::Result<()> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;

    let data = serde_json::to_vec_pretty(config)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

    fs::write(dir.join("config.json"), data)
}
