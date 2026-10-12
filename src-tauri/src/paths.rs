//! Menzies config and data directory helpers.

use crate::error::AppError;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

pub const APP_FOLDER: &str = "com.mariesta.menzies.dbms-desktop-menzies";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub config_dir: String,
    pub data_dir: String,
    pub catalog_db: String,
    pub logs_dir: String,
    pub temp_dir: String,
}

pub fn config_dir() -> Result<PathBuf, AppError> {
    let base = dirs::config_dir()
        .ok_or_else(|| AppError::coded("NO_CONFIG_DIR", "Home config directory unavailable"))?;
    Ok(base.join("menzies").join(APP_FOLDER))
}

pub fn data_dir() -> Result<PathBuf, AppError> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| AppError::coded("NO_DATA_DIR", "Home data directory unavailable"))?;
    Ok(base.join("menzies").join(APP_FOLDER))
}

pub fn ensure_app_dirs() -> Result<AppPaths, AppError> {
    let config = config_dir()?;
    let data = data_dir()?;
    let logs = data.join("logs");
    let temp = data.join("temp");
    fs::create_dir_all(&config)?;
    fs::create_dir_all(&data)?;
    fs::create_dir_all(&logs)?;
    fs::create_dir_all(&temp)?;
    let catalog_db = data.join("catalog.sqlite");
    Ok(AppPaths {
        config_dir: config.to_string_lossy().into_owned(),
        data_dir: data.to_string_lossy().into_owned(),
        catalog_db: catalog_db.to_string_lossy().into_owned(),
        logs_dir: logs.to_string_lossy().into_owned(),
        temp_dir: temp.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn app_paths() -> Result<AppPaths, String> {
    ensure_app_dirs().map_err(Into::into)
}
