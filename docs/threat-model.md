# Threat model: Database Manager

## Assets

- Database credentials (passwords, SSH secrets)
- Connection metadata and query history
- User data returned from remote databases
- Encrypted connection packs
- App catalog SQLite under the Menzies data dir

## Trust boundaries

| Boundary         | Notes                                               |
| ---------------- | --------------------------------------------------- |
| Webview          | Untrusted for privileged FS/network; talks IPC only |
| Rust core        | Owns drivers, keyring, catalog, tunnels             |
| Remote databases | Untrusted servers; TLS/SSH options available        |
| OS keyring       | Stores secrets; not written to JSON settings        |

## Capabilities

Minimal Tauri grants: window chrome, dialog open/save, updater, process restart. No shell allowlist. No open bind-all listeners for app APIs.

## Risks and controls

| Risk                           | Control                                            |
| ------------------------------ | -------------------------------------------------- |
| Credential theft from disk     | Keyring only; packs encrypted with passphrase      |
| Destructive SQL                | Confirm setting; IPC `DESTRUCTIVE_CONFIRM`         |
| Accidental prod edits          | Environment colors; confirmations                  |
| Path traversal on SQLite files | User-selected paths; parent existence checks       |
| Log leakage                    | Do not log passwords; structured errors with codes |
| Update trust                   | Tauri updater pubkey + GitHub release endpoint     |

## Out of scope here

Docker socket control and AI providers (not shipped).
