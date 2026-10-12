//! Multi-engine database drivers and live session pool.

mod mongo;
mod mssql;
mod mysql_family;
mod postgres;
mod sqlite;
mod ssh;

use crate::error::AppError;
use crate::models::{
    ApplyEditsRequest, ColumnInfo, ConnectionDraft, Engine, ErdGraph, ForeignKeyInfo, QueryRequest,
    QueryResult, SchemaCompareRequest, SchemaCompareResult, TablePageRequest, TreeNode,
};
use mongo::MongoSession;
use mssql::MssqlSession;
use mysql_family::MysqlSession;
use postgres::PostgresSession;
use sqlite::SqliteSession;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub enum LiveSession {
    Sqlite(SqliteSession),
    Postgres(PostgresSession),
    Mysql(MysqlSession),
    Mssql(MssqlSession),
    Mongo(MongoSession),
}

impl LiveSession {
    pub fn engine(&self) -> Engine {
        match self {
            Self::Sqlite(s) => s.engine(),
            Self::Postgres(s) => s.engine(),
            Self::Mysql(s) => s.engine(),
            Self::Mssql(s) => s.engine(),
            Self::Mongo(s) => s.engine(),
        }
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        match self {
            Self::Sqlite(s) => s.connection_id(),
            Self::Postgres(s) => s.connection_id(),
            Self::Mysql(s) => s.connection_id(),
            Self::Mssql(s) => s.connection_id(),
            Self::Mongo(s) => s.connection_id(),
        }
    }

    pub async fn list_tree(&self) -> Result<TreeNode, AppError> {
        match self {
            Self::Sqlite(s) => s.list_tree().await,
            Self::Postgres(s) => s.list_tree().await,
            Self::Mysql(s) => s.list_tree().await,
            Self::Mssql(s) => s.list_tree().await,
            Self::Mongo(s) => s.list_tree().await,
        }
    }

    pub async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<(Vec<ColumnInfo>, Vec<ForeignKeyInfo>), AppError> {
        match self {
            Self::Sqlite(s) => s.describe_table(schema, table).await,
            Self::Postgres(s) => s.describe_table(schema, table).await,
            Self::Mysql(s) => s.describe_table(schema, table).await,
            Self::Mssql(s) => s.describe_table(schema, table).await,
            Self::Mongo(s) => s.describe_table(schema, table).await,
        }
    }

    pub async fn execute(&self, request: QueryRequest) -> Result<QueryResult, AppError> {
        match self {
            Self::Sqlite(s) => s.execute(request).await,
            Self::Postgres(s) => s.execute(request).await,
            Self::Mysql(s) => s.execute(request).await,
            Self::Mssql(s) => s.execute(request).await,
            Self::Mongo(s) => s.execute(request).await,
        }
    }

    pub async fn table_page(&self, request: TablePageRequest) -> Result<QueryResult, AppError> {
        match self {
            Self::Sqlite(s) => s.table_page(request).await,
            Self::Postgres(s) => s.table_page(request).await,
            Self::Mysql(s) => s.table_page(request).await,
            Self::Mssql(s) => s.table_page(request).await,
            Self::Mongo(s) => s.table_page(request).await,
        }
    }

    pub async fn apply_edits(&self, request: ApplyEditsRequest) -> Result<u64, AppError> {
        match self {
            Self::Sqlite(s) => s.apply_edits(request).await,
            Self::Postgres(s) => s.apply_edits(request).await,
            Self::Mysql(s) => s.apply_edits(request).await,
            Self::Mssql(s) => s.apply_edits(request).await,
            Self::Mongo(s) => s.apply_edits(request).await,
        }
    }

    pub async fn erd(&self, schema: Option<&str>) -> Result<ErdGraph, AppError> {
        match self {
            Self::Sqlite(s) => s.erd(schema).await,
            Self::Postgres(s) => s.erd(schema).await,
            Self::Mysql(s) => s.erd(schema).await,
            Self::Mssql(s) => s.erd(schema).await,
            Self::Mongo(s) => s.erd(schema).await,
        }
    }

    pub async fn generate_ddl(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<String, AppError> {
        match self {
            Self::Sqlite(s) => s.generate_ddl(schema, table).await,
            Self::Postgres(s) => s.generate_ddl(schema, table).await,
            Self::Mysql(s) => s.generate_ddl(schema, table).await,
            Self::Mssql(s) => s.generate_ddl(schema, table).await,
            Self::Mongo(s) => s.generate_ddl(schema, table).await,
        }
    }

    pub async fn list_table_names(&self, schema: Option<&str>) -> Result<Vec<String>, AppError> {
        match self {
            Self::Sqlite(s) => s.list_table_names(schema).await,
            Self::Postgres(s) => s.list_table_names(schema).await,
            Self::Mysql(s) => s.list_table_names(schema).await,
            Self::Mssql(s) => s.list_table_names(schema).await,
            Self::Mongo(s) => s.list_table_names(schema).await,
        }
    }
}

#[derive(Default)]
pub struct SessionPool {
    sessions: HashMap<String, Arc<LiveSession>>,
}

impl SessionPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn open(&mut self, draft: &ConnectionDraft) -> Result<String, AppError> {
        let connection_id = draft
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let mut working = draft.clone();
        working.id = Some(connection_id.clone());

        // SSH tunnel: resolve effective host/port when enabled.
        if working.ssh.enabled {
            let (local_host, local_port) = ssh::open_tunnel(&working).await?;
            working.host = Some(local_host);
            working.port = Some(local_port);
        }

        let session = match working.engine {
            Engine::Sqlite => LiveSession::Sqlite(SqliteSession::connect(&working).await?),
            Engine::Postgresql => LiveSession::Postgres(PostgresSession::connect(&working).await?),
            Engine::Mysql | Engine::Mariadb => {
                LiveSession::Mysql(MysqlSession::connect(&working).await?)
            }
            Engine::Mssql => LiveSession::Mssql(MssqlSession::connect(&working).await?),
            Engine::Mongodb => LiveSession::Mongo(MongoSession::connect(&working).await?),
        };

        let session_id = Uuid::new_v4().to_string();
        self.sessions.insert(session_id.clone(), Arc::new(session));
        Ok(session_id)
    }

    pub fn get(&self, session_id: &str) -> Result<Arc<LiveSession>, AppError> {
        self.sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::coded("SESSION_MISSING", "Session is not open"))
    }

    pub fn close(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    #[allow(dead_code)]
    pub fn close_all(&mut self) {
        self.sessions.clear();
    }
}

pub type SharedPool = Arc<RwLock<SessionPool>>;

pub async fn test_connection(draft: &ConnectionDraft) -> Result<String, AppError> {
    let mut pool = SessionPool::new();
    let session_id = pool.open(draft).await?;
    let session = pool.get(&session_id)?;
    let tree = session.list_tree().await?;
    pool.close(&session_id);
    Ok(format!(
        "Connected to {} ({})",
        tree.name,
        draft.engine.as_str()
    ))
}

pub async fn compare_schemas(
    pool: &SessionPool,
    request: SchemaCompareRequest,
) -> Result<SchemaCompareResult, AppError> {
    let left = pool.get(&request.left_session_id)?;
    let right = pool.get(&request.right_session_id)?;
    let left_tables = left
        .list_table_names(request.left_schema.as_deref())
        .await?;
    let right_tables = right
        .list_table_names(request.right_schema.as_deref())
        .await?;
    let left_set: std::collections::BTreeSet<_> = left_tables.iter().cloned().collect();
    let right_set: std::collections::BTreeSet<_> = right_tables.iter().cloned().collect();
    let only_left: Vec<_> = left_set.difference(&right_set).cloned().collect();
    let only_right: Vec<_> = right_set.difference(&left_set).cloned().collect();
    let mut differing = Vec::new();
    let mut migration = String::new();
    for name in left_set.intersection(&right_set) {
        let (lc, _) = left
            .describe_table(request.left_schema.as_deref(), name)
            .await?;
        let (rc, _) = right
            .describe_table(request.right_schema.as_deref(), name)
            .await?;
        let lnames: Vec<_> = lc.iter().map(|c| c.name.clone()).collect();
        let rnames: Vec<_> = rc.iter().map(|c| c.name.clone()).collect();
        if lnames != rnames {
            differing.push(name.clone());
            migration.push_str(&format!(
                "-- Table {name} differs (columns left {:?} vs right {:?})\n",
                lnames, rnames
            ));
        }
    }
    for name in &only_left {
        migration.push_str(&format!("-- Missing on right: {name}\n"));
        if let Ok(ddl) = left
            .generate_ddl(request.left_schema.as_deref(), name)
            .await
        {
            migration.push_str(&ddl);
            migration.push('\n');
        }
    }
    Ok(SchemaCompareResult {
        only_left,
        only_right,
        differing,
        migration_sql: migration,
    })
}

pub fn is_destructive_sql(sql: &str) -> bool {
    let upper = sql.to_uppercase();
    ["DROP ", "TRUNCATE ", "ALTER ", "DELETE ", "UPDATE "]
        .iter()
        .any(|k| upper.contains(k))
}
