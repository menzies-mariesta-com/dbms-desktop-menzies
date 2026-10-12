use crate::error::AppError;
use crate::models::{
    ApplyEditsRequest, ColumnInfo, ConnectionDraft, Engine, ErdEdge, ErdGraph, ErdNode,
    ForeignKeyInfo, QueryRequest, QueryResult, TablePageRequest, TreeNode,
};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, Bson, Document},
    options::ClientOptions,
    Client,
};
use std::time::Instant;

pub struct MongoSession {
    connection_id: String,
    client: Client,
    default_db: String,
}

impl MongoSession {
    pub async fn connect(draft: &ConnectionDraft) -> Result<Self, AppError> {
        let uri = if let Some(uri) = draft.connection_uri.as_ref().filter(|s| !s.is_empty()) {
            uri.clone()
        } else {
            let host = draft.host.clone().unwrap_or_else(|| "127.0.0.1".into());
            let port = draft.port.unwrap_or(27017);
            let user = draft.username.clone().unwrap_or_default();
            let password = draft.password.clone().unwrap_or_default();
            let db = draft.database.clone().unwrap_or_else(|| "admin".into());
            if user.is_empty() {
                format!("mongodb://{host}:{port}/{db}")
            } else {
                format!("mongodb://{user}:{password}@{host}:{port}/{db}")
            }
        };
        let options = ClientOptions::parse(&uri)
            .await
            .map_err(|err| AppError::coded("MONGO_CONNECT", err.to_string()))?;
        let client = Client::with_options(options)
            .map_err(|err| AppError::coded("MONGO_CONNECT", err.to_string()))?;
        client
            .list_database_names(None, None)
            .await
            .map_err(|err| AppError::coded("MONGO_CONNECT", err.to_string()))?;
        Ok(Self {
            connection_id: draft.id.clone().unwrap_or_default(),
            client,
            default_db: draft.database.clone().unwrap_or_else(|| "admin".into()),
        })
    }

    pub fn engine(&self) -> Engine {
        Engine::Mongodb
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub async fn list_tree(&self) -> Result<TreeNode, AppError> {
        let names = self
            .client
            .list_database_names(None, None)
            .await
            .map_err(|err| AppError::coded("MONGO_INTROSPECT", err.to_string()))?;
        let mut children = Vec::new();
        for db_name in names {
            if db_name == "local" || db_name == "config" {
                continue;
            }
            let db = self.client.database(&db_name);
            let collections = db
                .list_collection_names(None)
                .await
                .map_err(|err| AppError::coded("MONGO_INTROSPECT", err.to_string()))?;
            let col_nodes = collections
                .into_iter()
                .map(|name| TreeNode {
                    id: format!("collection:{db_name}.{name}"),
                    kind: "collection".into(),
                    name: name.clone(),
                    engine: Engine::Mongodb,
                    children: vec![],
                    meta: serde_json::json!({ "schema": db_name }),
                })
                .collect();
            children.push(TreeNode {
                id: format!("database:{db_name}"),
                kind: "database".into(),
                name: db_name,
                engine: Engine::Mongodb,
                children: col_nodes,
                meta: serde_json::json!({}),
            });
        }
        Ok(TreeNode {
            id: format!("mongo:{}", self.connection_id),
            kind: "cluster".into(),
            name: "MongoDB".into(),
            engine: Engine::Mongodb,
            children,
            meta: serde_json::json!({}),
        })
    }

    pub async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<(Vec<ColumnInfo>, Vec<ForeignKeyInfo>), AppError> {
        let db_name = schema.unwrap_or(&self.default_db);
        let collection = self.client.database(db_name).collection::<Document>(table);
        let doc = collection
            .find_one(doc! {}, None)
            .await
            .map_err(|err| AppError::coded("MONGO_DESCRIBE", err.to_string()))?;
        let mut columns = Vec::new();
        if let Some(doc) = doc {
            for (key, value) in doc.iter() {
                columns.push(ColumnInfo {
                    name: key.clone(),
                    data_type: bson_type_name(value).into(),
                    nullable: true,
                    primary_key: key == "_id",
                    default_value: None,
                });
            }
        }
        Ok((columns, vec![]))
    }

    pub async fn execute(&self, request: QueryRequest) -> Result<QueryResult, AppError> {
        let started = Instant::now();
        let max_rows = request.max_rows.unwrap_or(500) as usize;
        let cmd: serde_json::Value = serde_json::from_str(&request.sql).map_err(|err| {
            AppError::coded(
                "MONGO_QUERY",
                format!("Mongo queries must be JSON commands: {err}"),
            )
        })?;
        let db_name = cmd
            .get("db")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_db);
        let collection_name = cmd
            .get("collection")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::coded("MONGO_QUERY", "collection is required"))?;
        let op = cmd.get("op").and_then(|v| v.as_str()).unwrap_or("find");
        let collection = self
            .client
            .database(db_name)
            .collection::<Document>(collection_name);

        match op {
            "find" => {
                let filter = cmd.get("filter").cloned().unwrap_or(serde_json::json!({}));
                let filter_doc = json_to_document(&filter)?;
                let mut cursor = collection
                    .find(filter_doc, None)
                    .await
                    .map_err(|err| AppError::coded("MONGO_EXEC", err.to_string()))?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor
                    .try_next()
                    .await
                    .map_err(|err| AppError::coded("MONGO_EXEC", err.to_string()))?
                {
                    if docs.len() >= max_rows {
                        break;
                    }
                    docs.push(doc);
                }
                Ok(docs_to_result(docs, started, request.explain))
            }
            "aggregate" => {
                let pipeline = cmd
                    .get("pipeline")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let stages: Result<Vec<Document>, AppError> =
                    pipeline.iter().map(json_to_document).collect();
                let mut cursor = collection
                    .aggregate(stages?, None)
                    .await
                    .map_err(|err| AppError::coded("MONGO_EXEC", err.to_string()))?;
                let mut docs = Vec::new();
                while let Some(doc) = cursor
                    .try_next()
                    .await
                    .map_err(|err| AppError::coded("MONGO_EXEC", err.to_string()))?
                {
                    if docs.len() >= max_rows {
                        break;
                    }
                    docs.push(doc);
                }
                Ok(docs_to_result(docs, started, request.explain))
            }
            "insert" => {
                let document = cmd
                    .get("document")
                    .ok_or_else(|| AppError::coded("MONGO_QUERY", "document is required"))?;
                let doc = json_to_document(document)?;
                collection
                    .insert_one(doc, None)
                    .await
                    .map_err(|err| AppError::coded("MONGO_EXEC", err.to_string()))?;
                Ok(QueryResult {
                    columns: vec!["ok".into()],
                    rows: vec![vec![serde_json::json!(1)]],
                    row_count: 1,
                    truncated: false,
                    duration_ms: started.elapsed().as_millis() as u64,
                    messages: vec!["Inserted 1 document".into()],
                    explain_text: None,
                })
            }
            other => Err(AppError::coded(
                "MONGO_QUERY",
                format!("Unsupported op: {other}"),
            )),
        }
    }

    pub async fn table_page(&self, request: TablePageRequest) -> Result<QueryResult, AppError> {
        let db = request.schema.unwrap_or_else(|| self.default_db.clone());
        let filter = request
            .filter_sql
            .as_ref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .unwrap_or(serde_json::json!({}));
        let cmd = serde_json::json!({
            "db": db,
            "collection": request.table,
            "op": "find",
            "filter": filter,
        });
        self.execute(QueryRequest {
            session_id: request.session_id,
            sql: cmd.to_string(),
            max_rows: Some(request.limit),
            explain: false,
            analyze: false,
        })
        .await
    }

    pub async fn apply_edits(&self, request: ApplyEditsRequest) -> Result<u64, AppError> {
        let mut total = 0u64;
        for edit in request.edits {
            let db = edit
                .schema
                .clone()
                .unwrap_or_else(|| self.default_db.clone());
            let collection = self
                .client
                .database(&db)
                .collection::<Document>(&edit.table);
            let pk = edit
                .pk
                .as_object()
                .ok_or_else(|| AppError::coded("EDIT_PK", "Primary key object required"))?;
            let id = pk
                .get("_id")
                .ok_or_else(|| AppError::coded("EDIT_PK", "Mongo edits require _id"))?;
            let filter = doc! { "_id": json_to_bson(id)? };
            let update = doc! { "$set": { edit.column.clone(): json_to_bson(&edit.new_value)? } };
            collection
                .update_one(filter, update, None)
                .await
                .map_err(|err| AppError::coded("MONGO_EDIT", err.to_string()))?;
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
        let db = schema.unwrap_or(&self.default_db);
        Ok(format!(
            "db.getSiblingDB('{db}').createCollection('{table}');"
        ))
    }

    pub async fn list_table_names(&self, schema: Option<&str>) -> Result<Vec<String>, AppError> {
        let db = schema.unwrap_or(&self.default_db);
        self.client
            .database(db)
            .list_collection_names(None)
            .await
            .map_err(|err| AppError::coded("MONGO_INTROSPECT", err.to_string()))
    }
}

fn docs_to_result(docs: Vec<Document>, started: Instant, explain: bool) -> QueryResult {
    let rows: Vec<Vec<serde_json::Value>> = docs
        .into_iter()
        .map(|d| {
            vec![serde_json::Value::String(
                serde_json::to_string_pretty(&document_to_json(&d)).unwrap_or_else(|_| "{}".into()),
            )]
        })
        .collect();
    QueryResult {
        row_count: rows.len() as u64,
        columns: vec!["document".into()],
        truncated: false,
        duration_ms: started.elapsed().as_millis() as u64,
        messages: vec![],
        explain_text: explain
            .then(|| "Mongo explain available via aggregate $explain stages".into()),
        rows,
    }
}

fn bson_type_name(value: &Bson) -> &'static str {
    match value {
        Bson::Double(_) => "double",
        Bson::String(_) => "string",
        Bson::Array(_) => "array",
        Bson::Document(_) => "object",
        Bson::Boolean(_) => "bool",
        Bson::Null => "null",
        Bson::Int32(_) => "int",
        Bson::Int64(_) => "long",
        Bson::ObjectId(_) => "objectId",
        Bson::DateTime(_) => "date",
        _ => "mixed",
    }
}

fn json_to_document(value: &serde_json::Value) -> Result<Document, AppError> {
    let bson = json_to_bson(value)?;
    match bson {
        Bson::Document(doc) => Ok(doc),
        _ => Err(AppError::coded(
            "MONGO_QUERY",
            "Expected a JSON object for Mongo document/filter",
        )),
    }
}

fn json_to_bson(value: &serde_json::Value) -> Result<Bson, AppError> {
    bson::to_bson(value).map_err(|err| AppError::coded("MONGO_BSON", err.to_string()))
}

fn document_to_json(doc: &Document) -> serde_json::Value {
    bson::from_bson::<serde_json::Value>(Bson::Document(doc.clone()))
        .unwrap_or(serde_json::json!({}))
}
