use crate::error::AppError;
use crate::models::{
    ApplyEditsRequest, ColumnInfo, ConnectionDraft, Engine, ErdEdge, ErdGraph, ErdNode,
    ForeignKeyInfo, QueryRequest, QueryResult, TablePageRequest, TreeNode,
};
use mysql_async::prelude::*;
use mysql_async::{OptsBuilder, Pool, Row, Value};
use std::time::Instant;

pub struct MysqlSession {
    connection_id: String,
    engine: Engine,
    pool: Pool,
    database: String,
}

impl MysqlSession {
    pub async fn connect(draft: &ConnectionDraft) -> Result<Self, AppError> {
        let host = draft.host.clone().unwrap_or_else(|| "127.0.0.1".into());
        let port = draft.port.unwrap_or(3306);
        let user = draft.username.clone().unwrap_or_else(|| "root".into());
        let password = draft.password.clone().unwrap_or_default();
        let db = draft.database.clone().unwrap_or_default();
        let opts = OptsBuilder::default()
            .ip_or_hostname(host)
            .tcp_port(port)
            .user(Some(user))
            .pass(Some(password))
            .db_name(if db.is_empty() {
                None
            } else {
                Some(db.clone())
            });
        let pool = Pool::new(opts);
        // Warm connection.
        let mut conn = pool
            .get_conn()
            .await
            .map_err(|err| AppError::coded("MYSQL_CONNECT", err.to_string()))?;
        conn.query_drop("SELECT 1")
            .await
            .map_err(|err| AppError::coded("MYSQL_CONNECT", err.to_string()))?;
        Ok(Self {
            connection_id: draft.id.clone().unwrap_or_default(),
            engine: draft.engine,
            pool,
            database: db,
        })
    }

    pub fn engine(&self) -> Engine {
        self.engine
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub async fn list_tree(&self) -> Result<TreeNode, AppError> {
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|err| AppError::coded("MYSQL_INTROSPECT", err.to_string()))?;
        let schemas: Vec<String> = if self.database.is_empty() {
            conn.query("SHOW DATABASES")
                .await
                .map_err(|err| AppError::coded("MYSQL_INTROSPECT", err.to_string()))?
        } else {
            vec![self.database.clone()]
        };
        let mut children = Vec::new();
        for schema in schemas {
            if matches!(
                schema.as_str(),
                "information_schema" | "performance_schema" | "mysql" | "sys"
            ) {
                continue;
            }
            let tables: Vec<String> = conn
                .exec(
                    "SELECT table_name FROM information_schema.tables WHERE table_schema = ? AND table_type='BASE TABLE' ORDER BY table_name",
                    (schema.clone(),),
                )
                .await
                .map_err(|err| AppError::coded("MYSQL_INTROSPECT", err.to_string()))?;
            let table_nodes = tables
                .into_iter()
                .map(|name| TreeNode {
                    id: format!("table:{schema}.{name}"),
                    kind: "table".into(),
                    name: name.clone(),
                    engine: self.engine,
                    children: vec![],
                    meta: serde_json::json!({ "schema": schema }),
                })
                .collect();
            children.push(TreeNode {
                id: format!("schema:{schema}"),
                kind: "schema".into(),
                name: schema,
                engine: self.engine,
                children: table_nodes,
                meta: serde_json::json!({}),
            });
        }
        Ok(TreeNode {
            id: format!("mysql:{}", self.connection_id),
            kind: "database".into(),
            name: if self.engine == Engine::Mariadb {
                "MariaDB".into()
            } else {
                "MySQL".into()
            },
            engine: self.engine,
            children,
            meta: serde_json::json!({}),
        })
    }

    pub async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<(Vec<ColumnInfo>, Vec<ForeignKeyInfo>), AppError> {
        let schema = schema
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.database.clone());
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|err| AppError::coded("MYSQL_DESCRIBE", err.to_string()))?;
        let rows: Vec<(String, String, String, Option<String>, String)> = conn
            .exec(
                "SELECT column_name, data_type, is_nullable, column_default, column_key
                 FROM information_schema.columns
                 WHERE table_schema = ? AND table_name = ?
                 ORDER BY ordinal_position",
                (schema.clone(), table.to_string()),
            )
            .await
            .map_err(|err| AppError::coded("MYSQL_DESCRIBE", err.to_string()))?;
        let columns = rows
            .into_iter()
            .map(
                |(name, data_type, nullable, default_value, key)| ColumnInfo {
                    primary_key: key == "PRI",
                    name,
                    data_type,
                    nullable: nullable == "YES",
                    default_value,
                },
            )
            .collect();
        let fk_rows: Vec<(String, String, String, String, String)> = conn
            .exec(
                "SELECT constraint_name, column_name, referenced_table_schema, referenced_table_name, referenced_column_name
                 FROM information_schema.key_column_usage
                 WHERE table_schema = ? AND table_name = ? AND referenced_table_name IS NOT NULL",
                (schema, table.to_string()),
            )
            .await
            .map_err(|err| AppError::coded("MYSQL_DESCRIBE", err.to_string()))?;
        let fks = fk_rows
            .into_iter()
            .map(
                |(name, col, ref_schema, ref_table, ref_col)| ForeignKeyInfo {
                    name,
                    columns: vec![col],
                    ref_schema: Some(ref_schema),
                    ref_table,
                    ref_columns: vec![ref_col],
                },
            )
            .collect();
        Ok((columns, fks))
    }

    pub async fn execute(&self, request: QueryRequest) -> Result<QueryResult, AppError> {
        let started = Instant::now();
        let max_rows = request.max_rows.unwrap_or(500) as usize;
        let sql = if request.explain {
            format!("EXPLAIN {}", request.sql)
        } else {
            request.sql.clone()
        };
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|err| AppError::coded("MYSQL_EXEC", err.to_string()))?;
        let upper = sql.trim().to_uppercase();
        if upper.starts_with("SELECT")
            || upper.starts_with("SHOW")
            || upper.starts_with("DESCRIBE")
            || upper.starts_with("EXPLAIN")
            || upper.starts_with("WITH")
        {
            let rows: Vec<Row> = conn
                .query(sql.clone())
                .await
                .map_err(|err| AppError::coded("MYSQL_EXEC", err.to_string()))?;
            let columns = if let Some(first) = rows.first() {
                (0..first.len())
                    .map(|i| {
                        first
                            .columns_ref()
                            .get(i)
                            .map(|c| c.name_str().to_string())
                            .unwrap_or_else(|| format!("c{i}"))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let mut rows_out = Vec::new();
            let mut truncated = false;
            let mut explain_lines = Vec::new();
            for row in rows {
                if rows_out.len() >= max_rows {
                    truncated = true;
                    break;
                }
                let values = row_to_json(&row, columns.len());
                if request.explain {
                    if let Some(serde_json::Value::String(s)) = values.first() {
                        explain_lines.push(s.clone());
                    }
                }
                rows_out.push(values);
            }
            Ok(QueryResult {
                row_count: rows_out.len() as u64,
                columns,
                rows: rows_out,
                truncated,
                duration_ms: started.elapsed().as_millis() as u64,
                messages: vec![],
                explain_text: request.explain.then(|| explain_lines.join("\n")),
            })
        } else {
            conn.query_drop(sql)
                .await
                .map_err(|err| AppError::coded("MYSQL_EXEC", err.to_string()))?;
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                row_count: 0,
                truncated: false,
                duration_ms: started.elapsed().as_millis() as u64,
                messages: vec!["Statement completed".into()],
                explain_text: None,
            })
        }
    }

    pub async fn table_page(&self, request: TablePageRequest) -> Result<QueryResult, AppError> {
        let schema = request
            .schema
            .clone()
            .unwrap_or_else(|| self.database.clone());
        let mut sql = if schema.is_empty() {
            format!("SELECT * FROM {}", quote_ident(&request.table))
        } else {
            format!(
                "SELECT * FROM {}.{}",
                quote_ident(&schema),
                quote_ident(&request.table)
            )
        };
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
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|err| AppError::coded("MYSQL_EDIT", err.to_string()))?;
        let mut total = 0u64;
        for edit in request.edits {
            let schema = edit.schema.as_deref().unwrap_or(self.database.as_str());
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
            conn.query_drop(sql)
                .await
                .map_err(|err| AppError::coded("MYSQL_EDIT", err.to_string()))?;
            total += 1;
        }
        Ok(total)
    }

    pub async fn erd(&self, schema: Option<&str>) -> Result<ErdGraph, AppError> {
        let schema = schema
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.database.clone());
        let names = self.list_table_names(Some(&schema)).await?;
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for name in names {
            let (cols, fks) = self.describe_table(Some(&schema), &name).await?;
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
        let schema = schema
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.database.clone());
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|err| AppError::coded("MYSQL_DDL", err.to_string()))?;
        let row: Option<Row> = conn
            .query_first(format!(
                "SHOW CREATE TABLE {}.{}",
                quote_ident(&schema),
                quote_ident(table)
            ))
            .await
            .map_err(|err| AppError::coded("MYSQL_DDL", err.to_string()))?;
        let row = row.ok_or_else(|| AppError::coded("MYSQL_DDL", "Table not found"))?;
        let ddl: String = row
            .get(1)
            .ok_or_else(|| AppError::coded("MYSQL_DDL", "Missing create statement"))?;
        Ok(ddl)
    }

    pub async fn list_table_names(&self, schema: Option<&str>) -> Result<Vec<String>, AppError> {
        let schema = schema
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.database.clone());
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|err| AppError::coded("MYSQL_INTROSPECT", err.to_string()))?;
        let tables: Vec<String> = conn
            .exec(
                "SELECT table_name FROM information_schema.tables WHERE table_schema = ? AND table_type='BASE TABLE' ORDER BY table_name",
                (schema,),
            )
            .await
            .map_err(|err| AppError::coded("MYSQL_INTROSPECT", err.to_string()))?;
        Ok(tables)
    }
}

fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

fn row_to_json(row: &Row, len: usize) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let v: Value = row.get(i).unwrap_or(Value::NULL);
        out.push(match v {
            Value::NULL => serde_json::Value::Null,
            Value::Bytes(b) => serde_json::Value::String(String::from_utf8_lossy(&b).into_owned()),
            Value::Int(i) => serde_json::json!(i),
            Value::UInt(u) => serde_json::json!(u),
            Value::Float(f) => serde_json::json!(f),
            Value::Double(f) => serde_json::json!(f),
            other => serde_json::Value::String(format!("{other:?}")),
        });
    }
    out
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
