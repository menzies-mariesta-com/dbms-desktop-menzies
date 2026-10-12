use crate::error::AppError;
use crate::models::{
    ApplyEditsRequest, ColumnInfo, ConnectionDraft, Engine, ErdEdge, ErdGraph, ErdNode,
    ForeignKeyInfo, QueryRequest, QueryResult, TablePageRequest, TreeNode,
};
use std::time::Instant;
use tokio_postgres::{NoTls, Row};

pub struct PostgresSession {
    connection_id: String,
    client: tokio_postgres::Client,
}

impl PostgresSession {
    pub async fn connect(draft: &ConnectionDraft) -> Result<Self, AppError> {
        let host = draft.host.clone().unwrap_or_else(|| "127.0.0.1".into());
        let port = draft.port.unwrap_or(5432);
        let user = draft.username.clone().unwrap_or_else(|| "postgres".into());
        let password = draft.password.clone().unwrap_or_default();
        let db = draft.database.clone().unwrap_or_else(|| "postgres".into());
        let conn_str = format!(
            "host={host} port={port} user={user} password={password} dbname={db} connect_timeout=10"
        );
        let (client, connection) = tokio_postgres::connect(&conn_str, NoTls)
            .await
            .map_err(|err| AppError::coded("PG_CONNECT", err.to_string()))?;
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                log::warn!("postgres connection closed: {err}");
            }
        });
        Ok(Self {
            connection_id: draft.id.clone().unwrap_or_default(),
            client,
        })
    }

    pub fn engine(&self) -> Engine {
        Engine::Postgresql
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub async fn list_tree(&self) -> Result<TreeNode, AppError> {
        let schemas = self
            .client
            .query(
                "SELECT schema_name FROM information_schema.schemata
                 WHERE schema_name NOT IN ('pg_catalog','information_schema','pg_toast')
                 ORDER BY schema_name",
                &[],
            )
            .await
            .map_err(|err| AppError::coded("PG_INTROSPECT", err.to_string()))?;
        let mut children = Vec::new();
        for schema_row in schemas {
            let schema: String = schema_row.get(0);
            let tables = self
                .client
                .query(
                    "SELECT table_name FROM information_schema.tables
                     WHERE table_schema = $1 AND table_type = 'BASE TABLE'
                     ORDER BY table_name",
                    &[&schema],
                )
                .await
                .map_err(|err| AppError::coded("PG_INTROSPECT", err.to_string()))?;
            let table_nodes = tables
                .into_iter()
                .map(|t| {
                    let name: String = t.get(0);
                    TreeNode {
                        id: format!("table:{schema}.{name}"),
                        kind: "table".into(),
                        name,
                        engine: Engine::Postgresql,
                        children: vec![],
                        meta: serde_json::json!({ "schema": schema }),
                    }
                })
                .collect();
            children.push(TreeNode {
                id: format!("schema:{schema}"),
                kind: "schema".into(),
                name: schema,
                engine: Engine::Postgresql,
                children: table_nodes,
                meta: serde_json::json!({}),
            });
        }
        Ok(TreeNode {
            id: format!("pg:{}", self.connection_id),
            kind: "database".into(),
            name: "PostgreSQL".into(),
            engine: Engine::Postgresql,
            children,
            meta: serde_json::json!({}),
        })
    }

    pub async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<(Vec<ColumnInfo>, Vec<ForeignKeyInfo>), AppError> {
        let schema = schema.unwrap_or("public");
        let cols = self
            .client
            .query(
                "SELECT column_name, data_type, is_nullable, column_default
                 FROM information_schema.columns
                 WHERE table_schema = $1 AND table_name = $2
                 ORDER BY ordinal_position",
                &[&schema, &table],
            )
            .await
            .map_err(|err| AppError::coded("PG_DESCRIBE", err.to_string()))?;
        let pk_rows = self
            .client
            .query(
                "SELECT a.attname
                 FROM pg_index i
                 JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
                 JOIN pg_class c ON c.oid = i.indrelid
                 JOIN pg_namespace n ON n.oid = c.relnamespace
                 WHERE i.indisprimary AND n.nspname = $1 AND c.relname = $2",
                &[&schema, &table],
            )
            .await
            .map_err(|err| AppError::coded("PG_DESCRIBE", err.to_string()))?;
        let pk: std::collections::HashSet<String> = pk_rows.into_iter().map(|r| r.get(0)).collect();
        let columns = cols
            .into_iter()
            .map(|row| {
                let name: String = row.get(0);
                ColumnInfo {
                    primary_key: pk.contains(&name),
                    name,
                    data_type: row.get(1),
                    nullable: row.get::<_, String>(2) == "YES",
                    default_value: row.get(3),
                }
            })
            .collect();
        let fk_rows = self
            .client
            .query(
                "SELECT tc.constraint_name, kcu.column_name, ccu.table_schema, ccu.table_name, ccu.column_name
                 FROM information_schema.table_constraints tc
                 JOIN information_schema.key_column_usage kcu
                   ON tc.constraint_name = kcu.constraint_name AND tc.table_schema = kcu.table_schema
                 JOIN information_schema.constraint_column_usage ccu
                   ON ccu.constraint_name = tc.constraint_name AND ccu.table_schema = tc.table_schema
                 WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = $1 AND tc.table_name = $2",
                &[&schema, &table],
            )
            .await
            .map_err(|err| AppError::coded("PG_DESCRIBE", err.to_string()))?;
        let fks = fk_rows
            .into_iter()
            .map(|row| ForeignKeyInfo {
                name: row.get(0),
                columns: vec![row.get(1)],
                ref_schema: row.get(2),
                ref_table: row.get(3),
                ref_columns: vec![row.get(4)],
            })
            .collect();
        Ok((columns, fks))
    }

    pub async fn execute(&self, request: QueryRequest) -> Result<QueryResult, AppError> {
        let started = Instant::now();
        let max_rows = request.max_rows.unwrap_or(500) as usize;
        let sql = if request.explain {
            if request.analyze {
                format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT) {}", request.sql)
            } else {
                format!("EXPLAIN {}", request.sql)
            }
        } else {
            request.sql.clone()
        };
        let rows = self
            .client
            .query(&sql, &[])
            .await
            .map_err(|err| AppError::coded("PG_EXEC", err.to_string()))?;
        if rows.is_empty() {
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                row_count: 0,
                truncated: false,
                duration_ms: started.elapsed().as_millis() as u64,
                messages: vec!["Statement completed".into()],
                explain_text: request.explain.then_some(sql),
            });
        }
        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        let mut out_rows = Vec::new();
        let mut truncated = false;
        for row in rows {
            if out_rows.len() >= max_rows {
                truncated = true;
                break;
            }
            out_rows.push(row_to_json(&row, columns.len()));
        }
        let explain_text = if request.explain {
            Some(
                out_rows
                    .iter()
                    .filter_map(|r| r.first().and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        } else {
            None
        };
        Ok(QueryResult {
            row_count: out_rows.len() as u64,
            columns,
            rows: out_rows,
            truncated,
            duration_ms: started.elapsed().as_millis() as u64,
            messages: vec![],
            explain_text,
        })
    }

    pub async fn table_page(&self, request: TablePageRequest) -> Result<QueryResult, AppError> {
        let schema = request.schema.unwrap_or_else(|| "public".into());
        let mut sql = format!(
            "SELECT * FROM {}.{}",
            quote_ident(&schema),
            quote_ident(&request.table)
        );
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
        let mut total = 0u64;
        for edit in request.edits {
            let schema = edit.schema.as_deref().unwrap_or("public");
            let pk = edit
                .pk
                .as_object()
                .ok_or_else(|| AppError::coded("EDIT_PK", "Primary key object required"))?;
            let mut wheres = Vec::new();
            let mut vals: Vec<&serde_json::Value> = Vec::new();
            vals.push(&edit.new_value);
            for (k, v) in pk {
                wheres.push(format!("{} = ${}", quote_ident(k), vals.len() + 1));
                vals.push(v);
            }
            let sql = format!(
                "UPDATE {}.{} SET {} = $1 WHERE {}",
                quote_ident(schema),
                quote_ident(&edit.table),
                quote_ident(&edit.column),
                wheres.join(" AND ")
            );
            // Parameterized dynamic SQL with tokio-postgres is awkward for mixed types;
            // use literal rendering with careful escaping for JSON scalars.
            let rendered = render_update(schema, &edit.table, &edit.column, &edit.new_value, pk)?;
            let n = self
                .client
                .execute(&rendered, &[])
                .await
                .map_err(|err| AppError::coded("PG_EDIT", err.to_string()))?;
            let _ = sql;
            total += n;
        }
        Ok(total)
    }

    pub async fn erd(&self, schema: Option<&str>) -> Result<ErdGraph, AppError> {
        let schema = schema.unwrap_or("public");
        let names = self.list_table_names(Some(schema)).await?;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for name in names {
            let (cols, fks) = self.describe_table(Some(schema), &name).await?;
            nodes.push(ErdNode {
                id: name.clone(),
                label: name.clone(),
                columns: cols.into_iter().map(|c| c.name).collect(),
            });
            for fk in fks {
                edges.push(ErdEdge {
                    from: name.clone(),
                    to: fk.ref_table,
                    label: fk.columns.join(","),
                });
            }
        }
        Ok(ErdGraph { nodes, edges })
    }

    pub async fn generate_ddl(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<String, AppError> {
        let (cols, _) = self.describe_table(schema, table).await?;
        let schema = schema.unwrap_or("public");
        let mut lines = Vec::new();
        for c in cols {
            let null = if c.nullable { "" } else { " NOT NULL" };
            lines.push(format!(
                "  {} {}{}",
                quote_ident(&c.name),
                c.data_type,
                null
            ));
        }
        Ok(format!(
            "CREATE TABLE {}.{} (\n{}\n);",
            quote_ident(schema),
            quote_ident(table),
            lines.join(",\n")
        ))
    }

    pub async fn list_table_names(&self, schema: Option<&str>) -> Result<Vec<String>, AppError> {
        let schema = schema.unwrap_or("public");
        let rows = self
            .client
            .query(
                "SELECT table_name FROM information_schema.tables
                 WHERE table_schema = $1 AND table_type = 'BASE TABLE' ORDER BY table_name",
                &[&schema],
            )
            .await
            .map_err(|err| AppError::coded("PG_INTROSPECT", err.to_string()))?;
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn row_to_json(row: &Row, len: usize) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let val: Option<String> = row.try_get::<_, String>(i).ok();
        if let Some(s) = val {
            out.push(serde_json::Value::String(s));
            continue;
        }
        if let Ok(v) = row.try_get::<_, i64>(i) {
            out.push(serde_json::json!(v));
            continue;
        }
        if let Ok(v) = row.try_get::<_, f64>(i) {
            out.push(serde_json::json!(v));
            continue;
        }
        if let Ok(v) = row.try_get::<_, bool>(i) {
            out.push(serde_json::json!(v));
            continue;
        }
        if row.try_get::<_, Option<String>>(i).ok().flatten().is_none() {
            // try null
        }
        out.push(
            row.try_get::<_, Option<String>>(i)
                .ok()
                .flatten()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
    }
    out
}

fn render_update(
    schema: &str,
    table: &str,
    column: &str,
    new_value: &serde_json::Value,
    pk: &serde_json::Map<String, serde_json::Value>,
) -> Result<String, AppError> {
    let mut wheres = Vec::new();
    for (k, v) in pk {
        wheres.push(format!("{} = {}", quote_ident(k), sql_literal(v)?));
    }
    Ok(format!(
        "UPDATE {}.{} SET {} = {} WHERE {}",
        quote_ident(schema),
        quote_ident(table),
        quote_ident(column),
        sql_literal(new_value)?,
        wheres.join(" AND ")
    ))
}

fn sql_literal(value: &serde_json::Value) -> Result<String, AppError> {
    match value {
        serde_json::Value::Null => Ok("NULL".into()),
        serde_json::Value::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.into()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::String(s) => Ok(format!("'{}'", s.replace('\'', "''"))),
        other => Err(AppError::coded(
            "EDIT_VALUE",
            format!("Unsupported value type: {other}"),
        )),
    }
}
