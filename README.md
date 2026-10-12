# Database Manager (`dbms-desktop-menzies`)

Linux-first Menzies desktop IDE for PostgreSQL, MySQL, MariaDB, SQL Server, SQLite, and MongoDB.

Identity: Lucide **DatabaseZap** + music water-block product icon. Wash UI chrome. GPLv3.

## Features

- Multi-engine connections with groups and environment colors
- OS keyring for passwords; encrypted connection packs
- Schema / collection navigator, query consoles, explain plans
- Editable data grid with pending DML review
- ER diagrams, schema compare, import/export, query history
- Titlebar menus (File, Connection, Query, Database, View, Tools, Help)
- No Docker dashboard and no AI assistant in this product

## Develop

```bash
npm install
npm run tauri -- dev
```

Web preview (no live drivers):

```bash
npm run dev
```

Dev server port: `2010` (override with `VITE_DEV_PORT`).

## Quality

```bash
npm run lint
npm run check
npm run lint
npm run test:unit -- --run
cd src-tauri && cargo fmt --all -- --check && cargo check
```

## Icon export

```bash
npm run icon:export
```

## Paths

- Config: `{config}/menzies/com.mariesta.menzies.dbms-desktop-menzies/`
- Data: `{data}/menzies/com.mariesta.menzies.dbms-desktop-menzies/` (catalog SQLite, logs, temp)
