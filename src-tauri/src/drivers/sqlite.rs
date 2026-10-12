use crate::error::AppError;
use crate::models::{
    ApplyEditsRequest, ColumnInfo, ConnectionDraft, Engine, ErdEdge, ErdGraph, ErdNode,
    ForeignKeyInfo, PendingCellEdit, QueryRequest, QueryResult, TablePageRequest, TreeNode,
};
use rusqlite::{params_from_iter, types::ValueRef, Connection};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

pub struct SqliteSession {
    connection_id: String,
    conn: Mutex<Connection>,
}

impl SqliteSession {
    pub async fn connect(draft: &ConnectionDraft) -> Result<Self, AppError> {
        let path = draft
            .file_path
            .clone()
            .ok_or_else(|| AppError::coded("SQLITE_PATH", "SQLite file path is required"))?;
        if path != ":memory:" && !Path::new(&path).exists() {
            // Allow creating new files when parent exists.
            if let Some(parent) = Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    return Err(AppError::coded(
                        "SQLITE_PATH",
                        format!("Parent directory does not exist: {}", parent.display()),
                    ));
                }
            }
        }
        let conn = Connection::open(&path)
            .map_err(|err| AppError::coded("SQLITE_CONNECT", err.to_string()))?;
        Ok(Self {
            connection_id: draft.id.clone().unwrap_or_default(),
            conn: Mutex::new(conn),
        })
    }

    pub fn engine(&self) -> Engine {
        Engine::Sqlite
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub async fn list_tree(&self) -> Result<TreeNode, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::coded("LOCK", "Connection lock poisoned"))?;
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|err| AppError::coded("SQLITE_INTROSPECT", err.to_string()))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|err| AppError::coded("SQLITE_INTROSPECT", err.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| AppError::coded("SQLITE_INTROSPECT", err.to_string()))?;
        let children = names
            .into_iter()
            .map(|name| TreeNode {
                id: format!("table:{name}"),
                kind: "table".into(),
                name: name.clone(),
                engine: Engine::Sqlite,
                children: vec![],
                meta: serde_json::json!({ "schema": "main" }),
            })
            .collect();
        Ok(TreeNode {
            id: format!("sqlite:{}", self.connection_id),
            kind: "database".into(),
            name: "main".into(),
            engine: Engine::Sqlite,
            children,
            meta: serde_json::json!({}),
        })
    }

    pub async fn describe_table(
        &self,
        _schema: Option<&str>,
        table: &str,
    ) -> Result<(Vec<ColumnInfo>, Vec<ForeignKeyInfo>), AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::coded("LOCK", "Connection lock poisoned"))?;
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({})", quote_ident(table)))
            .map_err(|err| AppError::coded("SQLITE_DESCRIBE", err.to_string()))?;
        let columns = stmt
            .query_map([], |row| {
                Ok(ColumnInfo {
                    name: row.get(1)?,
                    data_type: row.get::<_, String>(2).unwrap_or_else(|_| "TEXT".into()),
                    nullable: row.get::<_, i64>(3).unwrap_or(0) == 0,
                    primary_key: row.get::<_, i64>(5).unwrap_or(0) != 0,
                    default_value: row.get(4).ok(),
                })
            })
            .map_err(|err| AppError::coded("SQLITE_DESCRIBE", err.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| AppError::coded("SQLITE_DESCRIBE", err.to_string()))?;

        let mut fk_stmt = conn
            .prepare(&format!("PRAGMA foreign_key_list({})", quote_ident(table)))
            .map_err(|err| AppError::coded("SQLITE_DESCRIBE", err.to_string()))?;
        let fks = fk_stmt
            .query_map([], |row| {
                Ok(ForeignKeyInfo {
                    name: format!("fk_{}", row.get::<_, i64>(0).unwrap_or(0)),
                    columns: vec![row.get(3)?],
                    ref_schema: None,
                    ref_table: row.get(2)?,
                    ref_columns: vec![row.get(4)?],
                })
            })
            .map_err(|err| AppError::coded("SQLITE_DESCRIBE", err.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| AppError::coded("SQLITE_DESCRIBE", err.to_string()))?;
        Ok((columns, fks))
    }

    pub async fn execute(&self, request: QueryRequest) -> Result<QueryResult, AppError> {
        let started = Instant::now();
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::coded("LOCK", "Connection lock poisoned"))?;
        let max_rows = request.max_rows.unwrap_or(500) as usize;
        let sql = request.sql.trim();
        if request.explain {
            let explain_sql = if request.analyze {
                format!("EXPLAIN QUERY PLAN {sql}")
            } else {
                format!("EXPLAIN QUERY PLAN {sql}")
            };
            return run_select(
                &conn,
                &explain_sql,
                max_rows,
                started,
                Some(explain_sql.clone()),
            );
        }
        let upper = sql.to_uppercase();
        if upper.starts_with("SELECT")
            || upper.starts_with("WITH")
            || upper.starts_with("PRAGMA")
            || upper.starts_with("EXPLAIN")
        {
            run_select(&conn, sql, max_rows, started, None)
        } else {
            let changed = conn
                .execute(sql, [])
                .map_err(|err| AppError::coded("SQLITE_EXEC", err.to_string()))?;
            Ok(QueryResult {
                columns: vec!["rows_affected".into()],
                rows: vec![vec![serde_json::json!(changed)]],
                row_count: changed as u64,
                truncated: false,
                duration_ms: started.elapsed().as_millis() as u64,
                messages: vec![format!("{changed} row(s) affected")],
                explain_text: None,
            })
        }
    }

    pub async fn table_page(&self, request: TablePageRequest) -> Result<QueryResult, AppError> {
        let mut sql = format!("SELECT * FROM {}", quote_ident(&request.table));
        if let Some(filter) = request.filter_sql.as_ref().filter(|s| !s.trim().is_empty()) {
            sql.push_str(" WHERE ");
            sql.push_str(filter);
        }
        if let Some(order) = request.order_by.as_ref().filter(|s| !s.trim().is_empty()) {
            sql.push_str(" ORDER BY ");
            sql.push_str(order);
        }
        sql.push_str(&format!(
            " LIMIT {} OFFSET {}",
            request.limit, request.offset
        ));
        self.execute(QueryRequest {
            session_id: request.session_id,
            sql,
            max_rows: Some(request.limit),
            explain: false,
            analyze: false,
        })
        .await
    }

    pub async fn apply_edits(&self, request: ApplyEditsRequest) -> Result<u64, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::coded("LOCK", "Connection lock poisoned"))?;
        let mut total = 0u64;
        for edit in request.edits {
            total += apply_one(&conn, &edit)?;
        }
        Ok(total)
    }

    pub async fn erd(&self, _schema: Option<&str>) -> Result<ErdGraph, AppError> {
        let tree = self.list_tree().await?;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for child in &tree.children {
            let (cols, fks) = self.describe_table(None, &child.name).await?;
            nodes.push(ErdNode {
                id: child.name.clone(),
                label: child.name.clone(),
                columns: cols.into_iter().map(|c| c.name).collect(),
            });
            for fk in fks {
                edges.push(ErdEdge {
                    from: child.name.clone(),
                    to: fk.ref_table,
                    label: fk.columns.join(","),
                });
            }
        }
        Ok(ErdGraph { nodes, edges })
    }

    pub async fn generate_ddl(
        &self,
        _schema: Option<&str>,
        table: &str,
    ) -> Result<String, AppError> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| AppError::coded("LOCK", "Connection lock poisoned"))?;
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|err| AppError::coded("SQLITE_DDL", err.to_string()))?;
        Ok(format!("{sql};"))
    }

    pub async fn list_table_names(&self, _schema: Option<&str>) -> Result<Vec<String>, AppError> {
        let tree = self.list_tree().await?;
        Ok(tree.children.into_iter().map(|c| c.name).collect())
    }
}

fn run_select(
    conn: &Connection,
    sql: &str,
    max_rows: usize,
    started: Instant,
    explain_text: Option<String>,
) -> Result<QueryResult, AppError> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|err| AppError::coded("SQLITE_EXEC", err.to_string()))?;
    let columns: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let mut rows = Vec::new();
    let mut truncated = false;
    let mut query = stmt
        .query([])
        .map_err(|err| AppError::coded("SQLITE_EXEC", err.to_string()))?;
    while let Some(row) = query
        .next()
        .map_err(|err| AppError::coded("SQLITE_EXEC", err.to_string()))?
    {
        if rows.len() >= max_rows {
            truncated = true;
            break;
        }
        let mut values = Vec::with_capacity(columns.len());
        for idx in 0..columns.len() {
            values
                .push(value_to_json(row.get_ref(idx).map_err(|err| {
                    AppError::coded("SQLITE_EXEC", err.to_string())
                })?));
        }
        rows.push(values);
    }
    let row_count = rows.len() as u64;
    Ok(QueryResult {
        columns,
        rows,
        row_count,
        truncated,
        duration_ms: started.elapsed().as_millis() as u64,
        messages: vec![],
        explain_text,
    })
}

fn apply_one(conn: &Connection, edit: &PendingCellEdit) -> Result<u64, AppError> {
    let pk_obj = edit
        .pk
        .as_object()
        .ok_or_else(|| AppError::coded("EDIT_PK", "Primary key object required"))?;
    let mut where_parts = Vec::new();
    let mut params: Vec<rusqlite::types::Value> = Vec::new();
    params.push(json_to_value(&edit.new_value));
    for (k, v) in pk_obj {
        where_parts.push(format!("{} = ?", quote_ident(k)));
        params.push(json_to_value(v));
    }
    if where_parts.is_empty() {
        return Err(AppError::coded("EDIT_PK", "Primary key columns required"));
    }
    let sql = format!(
        "UPDATE {} SET {} = ? WHERE {}",
        quote_ident(&edit.table),
        quote_ident(&edit.column),
        where_parts.join(" AND ")
    );
    let n = conn
        .execute(&sql, params_from_iter(params))
        .map_err(|err| AppError::coded("SQLITE_EDIT", err.to_string()))?;
    Ok(n as u64)
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn value_to_json(value: ValueRef<'_>) -> serde_json::Value {
    match value {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => serde_json::json!(i),
        ValueRef::Real(f) => serde_json::json!(f),
        ValueRef::Text(t) => serde_json::Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => {
            use base64::{engine::general_purpose::STANDARD, Engine as _};
            serde_json::Value::String(format!("base64:{}", STANDARD.encode(b)))
        }
    }
}

fn json_to_value(value: &serde_json::Value) -> rusqlite::types::Value {
    match value {
        serde_json::Value::Null => rusqlite::types::Value::Null,
        serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(i64::from(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                rusqlite::types::Value::Real(f)
            } else {
                rusqlite::types::Value::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}
