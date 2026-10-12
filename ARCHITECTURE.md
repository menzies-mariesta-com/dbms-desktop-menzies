# Architecture: Database Manager

## Stack

- SvelteKit static SPA + Tauri 2
- Wash UI + Tailwind 4 + Paraglide + Zod
- Rust commands for drivers, catalog, keyring, packs

## IPC map

| Command                                                                         | Role                |
| ------------------------------------------------------------------------------- | ------------------- |
| `list_connections` / `save_connection` / `delete_connection` / `get_connection` | Connection catalog  |
| `test_connection` / `open_session` / `close_session`                            | Live sessions       |
| `list_tree` / `describe_table`                                                  | Introspection       |
| `run_query` / `table_page` / `apply_edits`                                      | Query and data      |
| `get_erd` / `generate_ddl` / `compare_schemas`                                  | Modeling            |
| `list_history` / bookmarks / settings                                           | Local catalog       |
| `export_results` / `export_backup` / connection packs / `result_diff`           | Transfer and safety |

## Drivers

Enum dispatch in `src-tauri/src/drivers/`:

- SQLite (`rusqlite`)
- PostgreSQL (`tokio-postgres`)
- MySQL / MariaDB (`mysql_async`)
- SQL Server (`tiberius`)
- MongoDB (`mongodb` 2.8 JSON command protocol)

SSH: local tunnel listener after bastion reachability check (`drivers/ssh.rs`). TLS modes exposed per engine where the client library supports them.

## Data dirs

`com.mariesta.menzies.dbms-desktop-menzies` under Menzies config and data roots. App catalog: `catalog.sqlite` via sqlx migrations.

## Frontend shell

`IdeShell.svelte`: navigator, workspace tabs, results, inspector. Titlebar menus pub/sub via `dbms-menu.ts`. Refresh via `dbms-refresh.ts`.

## Performance

Virtualize large grids in a later pass when row counts grow; current tables stream with max-row caps. Tear down sessions on destroy.
