//! Tauri command surface for Database Manager.

use crate::catalog::SharedCatalog;
use crate::drivers::{self, SharedPool};
use crate::error::AppError;
use crate::models::*;
use crate::pack;
use crate::paths::AppPaths;
use crate::{catalog, paths};
use std::sync::Arc;
use tauri::State;

pub struct AppState {
    pub catalog: SharedCatalog,
    pub pool: SharedPool,
}

#[tauri::command]
pub async fn list_connections(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ConnectionSummary>, String> {
    let catalog = state.catalog.read().await;
    catalog.list_connections().await.map_err(Into::into)
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, Arc<AppState>>,
    draft: ConnectionDraft,
) -> Result<ConnectionSummary, String> {
    let catalog = state.catalog.read().await;
    catalog.upsert_connection(draft).await.map_err(Into::into)
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let catalog = state.catalog.read().await;
    catalog.delete_connection(&id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn get_connection(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<ConnectionDraft, String> {
    let catalog = state.catalog.read().await;
    catalog.load_connection_draft(&id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn test_connection(draft: ConnectionDraft) -> Result<String, String> {
    drivers::test_connection(&draft).await.map_err(Into::into)
}

#[tauri::command]
pub async fn open_session(
    state: State<'_, Arc<AppState>>,
    connection_id: String,
) -> Result<String, String> {
    let catalog = state.catalog.read().await;
    let draft = catalog
        .load_connection_draft(&connection_id)
        .await
        .map_err(Into::<String>::into)?;
    drop(catalog);
    let mut pool = state.pool.write().await;
    pool.open(&draft).await.map_err(Into::into)
}

#[tauri::command]
pub async fn close_session(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<(), String> {
    let mut pool = state.pool.write().await;
    pool.close(&session_id);
    Ok(())
}

#[tauri::command]
pub async fn list_tree(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<TreeNode, String> {
    let pool = state.pool.read().await;
    let session = pool.get(&session_id).map_err(Into::<String>::into)?;
    session.list_tree().await.map_err(Into::into)
}

#[tauri::command]
pub async fn describe_table(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    schema: Option<String>,
    table: String,
) -> Result<(Vec<ColumnInfo>, Vec<ForeignKeyInfo>), String> {
    let pool = state.pool.read().await;
    let session = pool.get(&session_id).map_err(Into::<String>::into)?;
    session
        .describe_table(schema.as_deref(), &table)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn run_query(
    state: State<'_, Arc<AppState>>,
    request: QueryRequest,
    connection_id: Option<String>,
    confirm_destructive: bool,
) -> Result<QueryResult, String> {
    let settings = {
        let catalog = state.catalog.read().await;
        catalog.get_settings().await.map_err(Into::<String>::into)?
    };
    if settings.confirm_destructive
        && drivers::is_destructive_sql(&request.sql)
        && !confirm_destructive
    {
        return Err(AppError::coded(
            "DESTRUCTIVE_CONFIRM",
            "This statement may modify or destroy data. Confirm to continue.",
        )
        .into());
    }
    let pool = state.pool.read().await;
    let session = pool
        .get(&request.session_id)
        .map_err(Into::<String>::into)?;
    let engine = session.engine();
    let result = session
        .execute(request.clone())
        .await
        .map_err(Into::<String>::into)?;
    drop(pool);
    let catalog = state.catalog.read().await;
    let _ = catalog
        .add_history(
            connection_id.as_deref(),
            Some(engine),
            &request.sql,
            "ok",
            Some(result.duration_ms),
        )
        .await;
    Ok(result)
}

#[tauri::command]
pub async fn table_page(
    state: State<'_, Arc<AppState>>,
    request: TablePageRequest,
) -> Result<QueryResult, String> {
    let pool = state.pool.read().await;
    let session = pool
        .get(&request.session_id)
        .map_err(Into::<String>::into)?;
    session.table_page(request).await.map_err(Into::into)
}

#[tauri::command]
pub async fn apply_edits(
    state: State<'_, Arc<AppState>>,
    request: ApplyEditsRequest,
) -> Result<u64, String> {
    let pool = state.pool.read().await;
    let session = pool
        .get(&request.session_id)
        .map_err(Into::<String>::into)?;
    session.apply_edits(request).await.map_err(Into::into)
}

#[tauri::command]
pub async fn get_erd(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    schema: Option<String>,
) -> Result<ErdGraph, String> {
    let pool = state.pool.read().await;
    let session = pool.get(&session_id).map_err(Into::<String>::into)?;
    session.erd(schema.as_deref()).await.map_err(Into::into)
}

#[tauri::command]
pub async fn generate_ddl(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    schema: Option<String>,
    table: String,
) -> Result<String, String> {
    let pool = state.pool.read().await;
    let session = pool.get(&session_id).map_err(Into::<String>::into)?;
    session
        .generate_ddl(schema.as_deref(), &table)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn compare_schemas(
    state: State<'_, Arc<AppState>>,
    request: SchemaCompareRequest,
) -> Result<SchemaCompareResult, String> {
    let pool = state.pool.read().await;
    drivers::compare_schemas(&pool, request)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_history(
    state: State<'_, Arc<AppState>>,
    limit: Option<i64>,
) -> Result<Vec<HistoryEntry>, String> {
    let catalog = state.catalog.read().await;
    catalog
        .list_history(limit.unwrap_or(100))
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_bookmarks(state: State<'_, Arc<AppState>>) -> Result<Vec<BookmarkEntry>, String> {
    let catalog = state.catalog.read().await;
    catalog.list_bookmarks().await.map_err(Into::into)
}

#[tauri::command]
pub async fn add_bookmark(
    state: State<'_, Arc<AppState>>,
    connection_id: String,
    object_path: String,
    label: String,
) -> Result<BookmarkEntry, String> {
    let catalog = state.catalog.read().await;
    catalog
        .add_bookmark(&connection_id, &object_path, &label)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn delete_bookmark(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let catalog = state.catalog.read().await;
    catalog.delete_bookmark(&id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    let catalog = state.catalog.read().await;
    catalog.get_settings().await.map_err(Into::into)
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> Result<(), String> {
    let catalog = state.catalog.read().await;
    catalog.save_settings(&settings).await.map_err(Into::into)
}

#[tauri::command]
pub async fn export_results(request: ExportRequest) -> Result<String, String> {
    match request.format.as_str() {
        "csv" => {
            let mut out = request.columns.join(",");
            out.push('\n');
            for row in request.rows {
                let cells: Vec<String> = row
                    .into_iter()
                    .map(|v| match v {
                        serde_json::Value::Null => String::new(),
                        serde_json::Value::String(s) => {
                            format!("\"{}\"", s.replace('"', "\"\""))
                        }
                        other => other.to_string(),
                    })
                    .collect();
                out.push_str(&cells.join(","));
                out.push('\n');
            }
            Ok(out)
        }
        "json" => {
            let objects: Vec<serde_json::Value> = request
                .rows
                .into_iter()
                .map(|row| {
                    let mut map = serde_json::Map::new();
                    for (idx, col) in request.columns.iter().enumerate() {
                        map.insert(
                            col.clone(),
                            row.get(idx).cloned().unwrap_or(serde_json::Value::Null),
                        );
                    }
                    serde_json::Value::Object(map)
                })
                .collect();
            serde_json::to_string_pretty(&objects).map_err(|e| e.to_string())
        }
        "sql" => {
            let mut out = String::new();
            for row in request.rows {
                let values: Vec<String> = row
                    .into_iter()
                    .map(|v| match v {
                        serde_json::Value::Null => "NULL".into(),
                        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
                        other => other.to_string(),
                    })
                    .collect();
                out.push_str(&format!(
                    "INSERT INTO target ({}) VALUES ({});\n",
                    request.columns.join(", "),
                    values.join(", ")
                ));
            }
            Ok(out)
        }
        other => Err(AppError::coded("EXPORT_FORMAT", format!("Unknown format: {other}")).into()),
    }
}

#[tauri::command]
pub async fn export_connection_pack(
    state: State<'_, Arc<AppState>>,
    ids: Vec<String>,
    passphrase: String,
) -> Result<ConnectionPack, String> {
    let catalog = state.catalog.read().await;
    let mut drafts = Vec::new();
    for id in ids {
        drafts.push(
            catalog
                .load_connection_draft(&id)
                .await
                .map_err(Into::<String>::into)?,
        );
    }
    pack::export_pack(&drafts, &passphrase).map_err(Into::into)
}

#[tauri::command]
pub async fn import_connection_pack(
    state: State<'_, Arc<AppState>>,
    pack_data: ConnectionPack,
    passphrase: String,
) -> Result<Vec<ConnectionSummary>, String> {
    let drafts = pack::import_pack(&pack_data, &passphrase).map_err(Into::<String>::into)?;
    let catalog = state.catalog.read().await;
    let mut saved = Vec::new();
    for mut draft in drafts {
        draft.id = None;
        draft.password = draft.password.or(Some(String::new()));
        saved.push(
            catalog
                .upsert_connection(draft)
                .await
                .map_err(Into::<String>::into)?,
        );
    }
    Ok(saved)
}

#[tauri::command]
pub async fn export_backup(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let catalog = state.catalog.read().await;
    catalog.export_backup_json().await.map_err(Into::into)
}

#[tauri::command]
pub async fn result_diff(
    left: QueryResult,
    right: QueryResult,
) -> Result<serde_json::Value, String> {
    let left_set: std::collections::BTreeSet<String> = left
        .rows
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default())
        .collect();
    let right_set: std::collections::BTreeSet<String> = right
        .rows
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default())
        .collect();
    Ok(serde_json::json!({
        "onlyLeft": left_set.difference(&right_set).cloned().collect::<Vec<_>>(),
        "onlyRight": right_set.difference(&left_set).cloned().collect::<Vec<_>>(),
        "sharedCount": left_set.intersection(&right_set).count(),
    }))
}

#[tauri::command]
pub fn app_paths_cmd() -> Result<AppPaths, String> {
    paths::ensure_app_dirs().map_err(Into::into)
}

#[allow(dead_code)]
pub async fn init_state() -> Result<Arc<AppState>, AppError> {
    let catalog = catalog::Catalog::open().await?;
    Ok(Arc::new(AppState {
        catalog: Arc::new(tokio::sync::RwLock::new(catalog)),
        pool: Arc::new(tokio::sync::RwLock::new(drivers::SessionPool::new())),
    }))
}
