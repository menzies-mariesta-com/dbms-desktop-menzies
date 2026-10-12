//! OS keyring helpers for connection passwords.

use crate::error::AppError;
use crate::paths::APP_FOLDER;
use keyring::Entry;

fn entry(secret_ref: &str) -> Result<Entry, AppError> {
    Entry::new(APP_FOLDER, secret_ref)
        .map_err(|err| AppError::coded("KEYRING", format!("Keyring unavailable: {err}")))
}

pub fn secret_ref_for_connection(connection_id: &str) -> String {
    format!("connection:{connection_id}:password")
}

pub fn set_password(secret_ref: &str, password: &str) -> Result<(), AppError> {
    entry(secret_ref)?
        .set_password(password)
        .map_err(|err| AppError::coded("KEYRING_SET", format!("Could not store secret: {err}")))
}

pub fn get_password(secret_ref: &str) -> Result<Option<String>, AppError> {
    match entry(secret_ref)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(AppError::coded(
            "KEYRING_GET",
            format!("Could not read secret: {err}"),
        )),
    }
}

pub fn delete_password(secret_ref: &str) -> Result<(), AppError> {
    match entry(secret_ref)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(AppError::coded(
            "KEYRING_DELETE",
            format!("Could not delete secret: {err}"),
        )),
    }
}
