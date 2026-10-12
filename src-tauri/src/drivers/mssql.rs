use crate::error::AppError;
use crate::models::{
    ApplyEditsRequest, ColumnInfo, ConnectionDraft, Engine, ErdEdge, ErdGraph, ErdNode,
    ForeignKeyInfo, QueryRequest, QueryResult, TablePageRequest, TreeNode,
};
use std::time::Instant;
use tiberius::{AuthMethod, Client, Config, EncryptionLevel};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

pub struct MssqlSession {
    connection_id: String,
    client: tokio::sync::Mutex<Client<Compat<TcpStream>>>,
}

impl MssqlSession {
    pub async fn connect(draft: &ConnectionDraft) -> Result<Self, AppError> {
        let host = draft.host.clone().unwrap_or_else(|| "127.0.0.1".into());
        let port = draft.port.unwrap_or(1433);
        let user = draft.username.clone().unwrap_or_default();
        let password = draft.password.clone().unwrap_or_default();
        let db = draft.database.clone().unwrap_or_else(|| "master".into());
        let mut config = Config::new();
        config.host(host);
        config.port(port);
        config.authentication(AuthMethod::sql_server(user, password));
        config.database(db);
        match draft.ssl.mode.as_str() {
            "disable" => config.encryption(EncryptionLevel::Off),
            "require" => config.encryption(EncryptionLevel::Required),
            _ => config.encryption(EncryptionLevel::On),
        }
        config.trust_cert();
        let tcp = TcpStream::connect(config.get_addr())
            .await
            .map_err(|err| AppError::coded("MSSQL_CONNECT", err.to_string()))?;
        tcp.set_nodelay(true)
            .map_err(|err| AppError::coded("MSSQL_CONNECT", err.to_string()))?;
        let client = Client::connect(config, tcp.compat_write())
            .await
            .map_err(|err| AppError::coded("MSSQL_CONNECT", err.to_string()))?;
        Ok(Self {
            connection_id: draft.id.clone().unwrap_or_default(),
            client: tokio::sync::Mutex::new(client),
        })
    }

    pub fn engine(&self) -> Engine {
        Engine::Mssql
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub async fn list_tree(&self) -> Result<TreeNode, AppError> {
        let mut client = self.client.lock().await;
        let stream = client
            .query(
                "SELECT s.name AS schema_name, t.name AS table_name
                 FROM sys.tables t
                 JOIN sys.schemas s ON s.schema_id = t.schema_id
                 ORDER BY s.name, t.name",
                &[],
            )
            .await
            .map_err(|err| AppError::coded("MSSQL_INTROSPECT", err.to_string()))?;
        let rows = stream
            .into_first_result()
            .await
            .map_err(|err| AppError::coded("MSSQL_INTROSPECT", err.to_string()))?;
        let mut schema_map: std::collections::BTreeMap<String, Vec<TreeNode>> =
            std::collections::BTreeMap::new();
        for row in rows {
            let schema: &str = row.get(0).unwrap_or("");
            let table: &str = row.get(1).unwrap_or("");
            schema_map
                .entry(schema.to_string())
                .or_default()
                .push(TreeNode {
                    id: format!("table:{schema}.{table}"),
                    kind: "table".into(),
                    name: table.to_string(),
                    engine: Engine::Mssql,
                    children: vec![],
                    meta: serde_json::json!({ "schema": schema }),
                });
        }
        let children = schema_map
            .into_iter()
            .map(|(schema, tables)| TreeNode {
                id: format!("schema:{schema}"),
                kind: "schema".into(),
                name: schema,
                engine: Engine::Mssql,
                children: tables,
                meta: serde_json::json!({}),
            })
            .collect();
        Ok(TreeNode {
            id: format!("mssql:{}", self.connection_id),
            kind: "database".into(),
            name: "SQL Server".into(),
            engine: Engine::Mssql,
            children,
            meta: serde_json::json!({}),
        })
    }

    pub async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<(Vec<ColumnInfo>, Vec<ForeignKeyInfo>), AppError> {
        let schema = schema.unwrap_or("dbo");
        let mut client = self.client.lock().await;
        let sql = format!(
            "SELECT c.name, ty.name, c.is_nullable, c.is_identity
             FROM sys.columns c
             JOIN sys.types ty ON ty.user_type_id = c.user_type_id
             JOIN sys.tables t ON t.object_id = c.object_id
             JOIN sys.schemas s ON s.schema_id = t.schema_id
             WHERE s.name = N'{}' AND t.name = N'{}'
             ORDER BY c.column_id",
            schema.replace('\'', "''"),
            table.replace('\'', "''")
        );
        let stream = client
            .query(sql, &[])
            .await
            .map_err(|err| AppError::coded("MSSQL_DESCRIBE", err.to_string()))?;
        let rows = stream
            .into_first_result()
            .await
            .map_err(|err| AppError::coded("MSSQL_DESCRIBE", err.to_string()))?;
        let columns = rows
            .into_iter()
            .map(|row| {
                let name: &str = row.get(0).unwrap_or("");
                let data_type: &str = row.get(1).unwrap_or("nvarchar");
                let nullable: bool = row.get(2).unwrap_or(false);
                ColumnInfo {
                    name: name.to_string(),
                    data_type: data_type.to_string(),
                    nullable,
                    primary_key: false,
                    default_value: None,
                }
            })
            .collect();
        Ok((columns, vec![]))
    }

    pub async fn execute(&self, request: QueryRequest) -> Result<QueryResult, AppError> {
        let started = Instant::now();
        let max_rows = request.max_rows.unwrap_or(500) as usize;
        let sql = if request.explain {
            format!(
                "SET SHOWPLAN_TEXT ON; {}; SET SHOWPLAN_TEXT OFF;",
                request.sql
            )
        } else {
            request.sql.clone()
        };
        let mut client = self.client.lock().await;
        let stream = client
            .query(sql, &[])
            .await
            .map_err(|err| AppError::coded("MSSQL_EXEC", err.to_string()))?;
        let mut columns = Vec::new();
        let mut rows_out = Vec::new();
        let mut truncated = false;
        let mut result_sets = stream
            .into_results()
            .await
            .map_err(|err| AppError::coded("MSSQL_EXEC", err.to_string()))?;
        if let Some(rows) = result_sets.pop() {
            if let Some(first) = rows.first() {
                columns = first
                    .columns()
                    .iter()
                    .map(|c| c.name().to_string())
                    .collect();
            }
            for row in rows {
                if rows_out.len() >= max_rows {
                    truncated = true;
                    break;
                }
                let mut values = Vec::new();
                for i in 0..columns.len() {
                    let as_str: Option<&str> = row.get(i);
                    if let Some(s) = as_str {
                        values.push(serde_json::Value::String(s.to_string()));
                        continue;
                    }
                    let as_i: Option<i64> = row.get(i);
                    if let Some(v) = as_i {
                        values.push(serde_json::json!(v));
                        continue;
                    }
                    values.push(serde_json::Value::Null);
                }
                rows_out.push(values);
            }
        }
        Ok(QueryResult {
            row_count: rows_out.len() as u64,
            columns,
            rows: rows_out,
            truncated,
            duration_ms: started.elapsed().as_millis() as u64,
            messages: vec![],
            explain_text: request.explain.then(|| "SHOWPLAN requested".into()),
        })
    }

    pub async fn table_page(&self, request: TablePageRequest) -> Result<QueryResult, AppError> {
        let schema = request.schema.unwrap_or_else(|| "dbo".into());
        let mut sql = format!(
            "SELECT * FROM {}.{}",
            quote_ident(&schema),
            quote_ident(&request.table)
        );
        if let Some(filter) = request.filter_sql.as_ref().filter(|s| !s.trim().is_empty()) {
            sql.push_str(" WHERE ");
            sql.push_str(filter);
        }
        sql.push_str(&format!(
            " ORDER BY (SELECT NULL) OFFSET {} ROWS FETCH NEXT {} ROWS ONLY",
            request.offset, request.limit
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
        let mut client = self.client.lock().await;
        let mut total = 0u64;
        for edit in request.edits {
            let schema = edit.schema.as_deref().unwrap_or("dbo");
            let pk = edit
                .pk
                .as_object()
                .ok_or_else(|| AppError::coded("EDIT_PK", "Primary key object required"))?;
            let mut wheres = Vec::new();
            for (k, v) in pk {
                wheres.push(format!("{} = {}", quote_ident(k), sql_literal(v)?));
            }
            let sql = format!(
                "UPDATE {}.{} SET {} = {} WHERE {}",
                quote_ident(schema),
                quote_ident(&edit.table),
                quote_ident(&edit.column),
                sql_literal(&edit.new_value)?,
                wheres.join(" AND ")
            );
            client
                .execute(sql, &[])
                .await
                .map_err(|err| AppError::coded("MSSQL_EDIT", err.to_string()))?;
            total += 1;
        }
        Ok(total)
    }

    pub async fn erd(&self, schema: Option<&str>) -> Result<ErdGraph, AppError> {
        let names = self.list_table_names(schema).await?;
        let mut nodes = Vec::new();
        for name in names {
            let (cols, _) = self.describe_table(schema, &name).await?;
            nodes.push(ErdNode {
                id: name.clone(),
                label: name,
                columns: cols.into_iter().map(|c| c.name).collect(),
            });
        }
        Ok(ErdGraph {
            nodes,
            edges: Vec::<ErdEdge>::new(),
        })
    }

    pub async fn generate_ddl(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<String, AppError> {
        let (cols, _) = self.describe_table(schema, table).await?;
        let schema = schema.unwrap_or("dbo");
        let lines: Vec<_> = cols
            .into_iter()
            .map(|c| {
                format!(
                    "  {} {}{}",
                    quote_ident(&c.name),
                    c.data_type,
                    if c.nullable { "" } else { " NOT NULL" }
                )
            })
            .collect();
        Ok(format!(
            "CREATE TABLE {}.{} (\n{}\n);",
            quote_ident(schema),
            quote_ident(table),
            lines.join(",\n")
        ))
    }

    pub async fn list_table_names(&self, schema: Option<&str>) -> Result<Vec<String>, AppError> {
        let tree = self.list_tree().await?;
        let schema = schema.unwrap_or("dbo");
        Ok(tree
            .children
            .into_iter()
            .filter(|s| s.name == schema)
            .flat_map(|s| s.children.into_iter().map(|t| t.name))
            .collect())
    }
}

fn quote_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

fn sql_literal(value: &serde_json::Value) -> Result<String, AppError> {
    match value {
        serde_json::Value::Null => Ok("NULL".into()),
        serde_json::Value::Bool(b) => Ok(if *b { "1" } else { "0" }.into()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::String(s) => Ok(format!("'{}'", s.replace('\'', "''"))),
        other => Err(AppError::coded(
            "EDIT_VALUE",
            format!("Unsupported value type: {other}"),
        )),
    }
}
