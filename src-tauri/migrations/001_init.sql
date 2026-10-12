CREATE TABLE IF NOT EXISTS connections (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    engine TEXT NOT NULL,
    group_name TEXT NOT NULL DEFAULT 'Default',
    env_color TEXT NOT NULL DEFAULT 'neutral',
    host TEXT,
    port INTEGER,
    database_name TEXT,
    username TEXT,
    file_path TEXT,
    connection_uri TEXT,
    ssl_json TEXT NOT NULL DEFAULT '{}',
    ssh_json TEXT NOT NULL DEFAULT '{}',
    extra_json TEXT NOT NULL DEFAULT '{}',
    secret_ref TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS query_history (
    id TEXT PRIMARY KEY NOT NULL,
    connection_id TEXT,
    engine TEXT,
    sql_text TEXT NOT NULL,
    status TEXT NOT NULL,
    duration_ms INTEGER,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS bookmarks (
    id TEXT PRIMARY KEY NOT NULL,
    connection_id TEXT NOT NULL,
    object_path TEXT NOT NULL,
    label TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS console_local_history (
    id TEXT PRIMARY KEY NOT NULL,
    console_id TEXT NOT NULL,
    sql_text TEXT NOT NULL,
    created_at TEXT NOT NULL
);
