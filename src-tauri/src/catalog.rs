//! Local app catalog (SQLite via sqlx).

use crate::error::AppError;
use crate::models::{
    AppSettings, BookmarkEntry, ConnectionDraft, ConnectionSummary, Engine, EnvColor, HistoryEntry,
    SshTunnelConfig, SslConfig,
};
use crate::paths::{self, ensure_app_dirs};
use crate::secrets;
use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct Catalog {
    pool: SqlitePool,
}

impl Catalog {
    pub async fn open() -> Result<Self, AppError> {
        let paths = ensure_app_dirs()?;
        let url = format!("sqlite:{}?mode=rwc", paths.catalog_db);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        let catalog = Self { pool };
        catalog.migrate().await?;
        Ok(catalog)
    }

    async fn migrate(&self) -> Result<(), AppError> {
        let sql = include_str!("../migrations/001_init.sql");
        sqlx::raw_sql(sql).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_connections(&self) -> Result<Vec<ConnectionSummary>, AppError> {
        let rows = sqlx::query(
            r#"SELECT id, name, engine, group_name, env_color, host, port, database_name,
                      username, file_path, secret_ref, ssl_json, ssh_json, created_at, updated_at
               FROM connections ORDER BY group_name, name"#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let engine_str: String = row.try_get("engine")?;
            let env_str: String = row.try_get("env_color")?;
            let secret_ref: Option<String> = row.try_get("secret_ref")?;
            let ssl_json: String = row.try_get("ssl_json")?;
            let ssh_json: String = row.try_get("ssh_json")?;
            let ssl: SslConfig = serde_json::from_str(&ssl_json).unwrap_or_default();
            let ssh: SshTunnelConfig = serde_json::from_str(&ssh_json).unwrap_or_default();
            out.push(ConnectionSummary {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                engine: parse_engine(&engine_str)?,
                group_name: row.try_get("group_name")?,
                env_color: parse_env(&env_str),
                host: row.try_get("host")?,
                port: row.try_get::<Option<i64>, _>("port")?.map(|p| p as u16),
                database: row.try_get("database_name")?,
                username: row.try_get("username")?,
                file_path: row.try_get("file_path")?,
                has_password: secret_ref.is_some(),
                ssl_mode: ssl.mode,
                ssh_enabled: ssh.enabled,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            });
        }
        Ok(out)
    }

    pub async fn upsert_connection(
        &self,
        draft: ConnectionDraft,
    ) -> Result<ConnectionSummary, AppError> {
        let now = Utc::now().to_rfc3339();
        let id = draft
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let secret_ref = secrets::secret_ref_for_connection(&id);
        if let Some(password) = draft.password.as_ref() {
            if password.is_empty() {
                secrets::delete_password(&secret_ref)?;
            } else {
                secrets::set_password(&secret_ref, password)?;
            }
        }
        let has_password = secrets::get_password(&secret_ref)?.is_some();
        let ssl_json = serde_json::to_string(&draft.ssl)?;
        let ssh_json = serde_json::to_string(&draft.ssh)?;
        let extra_json = serde_json::to_string(&draft.extra_options)?;

        sqlx::query(
            r#"INSERT INTO connections (
                id, name, engine, group_name, env_color, host, port, database_name, username,
                file_path, connection_uri, ssl_json, ssh_json, extra_json, secret_ref, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, engine=excluded.engine, group_name=excluded.group_name,
                env_color=excluded.env_color, host=excluded.host, port=excluded.port,
                database_name=excluded.database_name, username=excluded.username,
                file_path=excluded.file_path, connection_uri=excluded.connection_uri,
                ssl_json=excluded.ssl_json, ssh_json=excluded.ssh_json, extra_json=excluded.extra_json,
                secret_ref=excluded.secret_ref, updated_at=excluded.updated_at"#,
        )
        .bind(&id)
        .bind(&draft.name)
        .bind(draft.engine.as_str())
        .bind(&draft.group_name)
        .bind(env_as_str(draft.env_color))
        .bind(&draft.host)
        .bind(draft.port.map(|p| p as i64))
        .bind(&draft.database)
        .bind(&draft.username)
        .bind(&draft.file_path)
        .bind(&draft.connection_uri)
        .bind(&ssl_json)
        .bind(&ssh_json)
        .bind(&extra_json)
        .bind(if has_password { Some(secret_ref) } else { None })
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.list_connections()
            .await?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| AppError::coded("CONNECTION_MISSING", "Saved connection not found"))
    }

    pub async fn delete_connection(&self, id: &str) -> Result<(), AppError> {
        let secret_ref = secrets::secret_ref_for_connection(id);
        secrets::delete_password(&secret_ref)?;
        sqlx::query("DELETE FROM connections WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_connection_draft(&self, id: &str) -> Result<ConnectionDraft, AppError> {
        let row = sqlx::query(
            r#"SELECT id, name, engine, group_name, env_color, host, port, database_name, username,
                      file_path, connection_uri, ssl_json, ssh_json, extra_json, secret_ref
               FROM connections WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::coded("CONNECTION_MISSING", "Connection not found"))?;

        let secret_ref: Option<String> = row.try_get("secret_ref")?;
        let password = if let Some(r) = secret_ref {
            secrets::get_password(&r)?
        } else {
            None
        };
        let ssl_json: String = row.try_get("ssl_json")?;
        let ssh_json: String = row.try_get("ssh_json")?;
        let extra_json: String = row.try_get("extra_json")?;
        Ok(ConnectionDraft {
            id: Some(row.try_get("id")?),
            name: row.try_get("name")?,
            engine: parse_engine(&row.try_get::<String, _>("engine")?)?,
            group_name: row.try_get("group_name")?,
            env_color: parse_env(&row.try_get::<String, _>("env_color")?),
            host: row.try_get("host")?,
            port: row.try_get::<Option<i64>, _>("port")?.map(|p| p as u16),
            database: row.try_get("database_name")?,
            username: row.try_get("username")?,
            password,
            file_path: row.try_get("file_path")?,
            connection_uri: row.try_get("connection_uri")?,
            ssl: serde_json::from_str(&ssl_json).unwrap_or_default(),
            ssh: serde_json::from_str(&ssh_json).unwrap_or_default(),
            extra_options: serde_json::from_str(&extra_json).unwrap_or(serde_json::json!({})),
        })
    }

    pub async fn add_history(
        &self,
        connection_id: Option<&str>,
        engine: Option<Engine>,
        sql_text: &str,
        status: &str,
        duration_ms: Option<u64>,
    ) -> Result<(), AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"INSERT INTO query_history (id, connection_id, engine, sql_text, status, duration_ms, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(connection_id)
        .bind(engine.map(|e| e.as_str().to_string()))
        .bind(sql_text)
        .bind(status)
        .bind(duration_ms.map(|d| d as i64))
        .bind(&now)
        .execute(&self.pool)
        .await?;
        // Cap history growth.
        sqlx::query(
            r#"DELETE FROM query_history WHERE id NOT IN (
                SELECT id FROM query_history ORDER BY created_at DESC LIMIT 2000
            )"#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_history(&self, limit: i64) -> Result<Vec<HistoryEntry>, AppError> {
        let rows = sqlx::query(
            r#"SELECT id, connection_id, engine, sql_text, status, duration_ms, created_at
               FROM query_history ORDER BY created_at DESC LIMIT ?"#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for row in rows {
            let engine_str: Option<String> = row.try_get("engine")?;
            out.push(HistoryEntry {
                id: row.try_get("id")?,
                connection_id: row.try_get("connection_id")?,
                engine: engine_str.as_deref().and_then(|s| parse_engine(s).ok()),
                sql_text: row.try_get("sql_text")?,
                status: row.try_get("status")?,
                duration_ms: row
                    .try_get::<Option<i64>, _>("duration_ms")?
                    .map(|d| d as u64),
                created_at: row.try_get("created_at")?,
            });
        }
        Ok(out)
    }

    pub async fn list_bookmarks(&self) -> Result<Vec<BookmarkEntry>, AppError> {
        let rows = sqlx::query(
            "SELECT id, connection_id, object_path, label, created_at FROM bookmarks ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| BookmarkEntry {
                id: row.get("id"),
                connection_id: row.get("connection_id"),
                object_path: row.get("object_path"),
                label: row.get("label"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    pub async fn add_bookmark(
        &self,
        connection_id: &str,
        object_path: &str,
        label: &str,
    ) -> Result<BookmarkEntry, AppError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO bookmarks (id, connection_id, object_path, label, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(connection_id)
        .bind(object_path)
        .bind(label)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(BookmarkEntry {
            id,
            connection_id: connection_id.into(),
            object_path: object_path.into(),
            label: label.into(),
            created_at: now,
        })
    }

    pub async fn delete_bookmark(&self, id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM bookmarks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_settings(&self) -> Result<AppSettings, AppError> {
        let row = sqlx::query("SELECT value_json FROM settings WHERE key = 'app'")
            .fetch_optional(&self.pool)
            .await?;
        if let Some(row) = row {
            let raw: String = row.try_get("value_json")?;
            Ok(serde_json::from_str(&raw).unwrap_or_default())
        } else {
            Ok(AppSettings::default())
        }
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> Result<(), AppError> {
        let raw = serde_json::to_string(settings)?;
        sqlx::query(
            r#"INSERT INTO settings (key, value_json) VALUES ('app', ?)
               ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json"#,
        )
        .bind(raw)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn export_backup_json(&self) -> Result<String, AppError> {
        let connections = self.list_connections().await?;
        let history = self.list_history(500).await?;
        let bookmarks = self.list_bookmarks().await?;
        let settings = self.get_settings().await?;
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "exportedAt": Utc::now().to_rfc3339(),
            "connections": connections,
            "history": history,
            "bookmarks": bookmarks,
            "settings": settings,
        }))?)
    }

    #[allow(dead_code)]
    pub fn settings_path_hint() -> Result<String, AppError> {
        Ok(paths::config_dir()?
            .join("settings.json")
            .to_string_lossy()
            .into_owned())
    }
}

pub type SharedCatalog = Arc<RwLock<Catalog>>;

fn parse_engine(value: &str) -> Result<Engine, AppError> {
    match value {
        "postgresql" => Ok(Engine::Postgresql),
        "mysql" => Ok(Engine::Mysql),
        "mariadb" => Ok(Engine::Mariadb),
        "mssql" => Ok(Engine::Mssql),
        "sqlite" => Ok(Engine::Sqlite),
        "mongodb" => Ok(Engine::Mongodb),
        other => Err(AppError::coded(
            "ENGINE_UNKNOWN",
            format!("Unknown engine: {other}"),
        )),
    }
}

fn parse_env(value: &str) -> EnvColor {
    match value {
        "production" => EnvColor::Production,
        "staging" => EnvColor::Staging,
        "development" => EnvColor::Development,
        _ => EnvColor::Neutral,
    }
}

fn env_as_str(value: EnvColor) -> &'static str {
    match value {
        EnvColor::Production => "production",
        EnvColor::Staging => "staging",
        EnvColor::Development => "development",
        EnvColor::Neutral => "neutral",
    }
}
