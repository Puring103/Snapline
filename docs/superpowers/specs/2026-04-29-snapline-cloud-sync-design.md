# Snapline M2 Cloud Sync Design

## Overview

M2 adds optional cloud sync to Snapline while keeping the product local-first. Editing and saving notes must remain fast and reliable when the network is unavailable. Sync runs in the background and never blocks the core note-taking loop.

The sync backend is open source and self-hostable. The official hosted service and self-hosted deployments should use the same server code and protocol.

M2 includes text notes, note metadata, soft deletes, pinned state, and pasted image assets. It does not include real-time collaboration, CRDT merging, team workspaces, end-to-end encryption, or mobile clients.

## Goals

- Preserve local-first behavior.
- Support account-based sync across multiple desktop devices.
- Provide an open source backend that can be self-hosted.
- Support self-hosted deployments with Docker Compose.
- Sync pasted image assets as well as Markdown content.
- Keep the sync protocol small and understandable.
- Make conflicts explicit by creating conflict copies instead of silently overwriting content.

## Non-Goals

- Real-time collaborative editing.
- Character-level merge or CRDT.
- Team accounts, shared notebooks, or permissions.
- End-to-end encryption.
- S3 or MinIO storage in M2.
- Admin dashboard.
- Password reset email flow.
- Attachment management UI beyond images referenced from notes.

## Architecture

```text
Snapline Desktop
  SQLite
  change_queue
  sync_state
  local assets
  sync-client
      |
      | HTTPS / Sync API
      v
Snapline Sync Server
  Axum
  PostgreSQL
  LocalFsAssetStore
```

The client keeps SQLite as the source of immediate user experience. All edits are saved locally first, then queued for background upload. The server stores account-scoped note state, device records, change log events, and image asset metadata.

The server is a single Axum service backed by PostgreSQL. M2 stores asset bytes on the server filesystem through a small `AssetStore` abstraction. Future S3 or MinIO support should be added by implementing the same interface, without changing the client protocol.

## Client Data Model

The existing `notes` table should be extended with sync metadata:

- `server_version INTEGER NOT NULL DEFAULT 0`
- `last_modified_by_device TEXT`
- `is_conflict_copy INTEGER NOT NULL DEFAULT 0`
- `source_note_id TEXT`

`server_version` is the last accepted server version known by this client. Local-only notes use version `0` until uploaded.

`last_modified_by_device` records the device that produced the latest local version.

`is_conflict_copy` marks a local note created to preserve an edit rejected by the server due to version conflict.

`source_note_id` points from a conflict copy to the original note.

M2 adds `change_queue`:

```text
change_queue
- id
- note_id
- op_type
- base_version
- payload_json
- requires_asset_upload
- queued_at
- retry_count
- last_error
```

`op_type` supports `upsert_note`, `delete_note`, and `asset_upload`.

M2 adds `sync_state`:

```text
sync_state
- account_id
- device_id
- server_base_url
- server_cursor
- access_token
- last_sync_at
- last_success_at
```

Access token storage should use platform-secure storage when available. If secure storage is deferred, the implementation must make that limitation explicit in the UI and documentation.

## Asset Model

Client assets continue to use stable local paths:

```text
data/assets/notes/<note_id>/<asset_id>.png
```

Markdown stores relative references:

```markdown
![](assets/notes/<note_id>/<asset_id>.png)
```

The client generates `asset_id` locally when an image is pasted. It computes `sha256` before upload. Asset upload is idempotent: if the server already has an asset with the same account, asset id, and sha256, the upload succeeds without rewriting bytes.

The server stores M2 asset files under:

```text
server-data/assets/accounts/<account_id>/notes/<note_id>/<asset_id>.png
```

PostgreSQL stores asset metadata:

```text
assets
- id
- account_id
- note_id
- content_type
- byte_size
- sha256
- storage_key
- created_at
- deleted_at
```

Server code exposes an internal asset store boundary:

```rust
trait AssetStore {
    async fn put(&self, key: &str, bytes: Bytes) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn delete(&self, key: &str) -> Result<()>;
}
```

M2 implements only `LocalFsAssetStore`.

## Server Data Model

The server owns account-scoped global state.

```text
accounts
- id
- email
- password_hash
- created_at
- disabled_at
```

```text
devices
- id
- account_id
- name
- created_at
- last_seen_at
```

```text
notes
- id
- account_id
- title
- content_md
- pinned
- created_at
- updated_at
- deleted_at
- version
- last_modified_by_device
```

```text
change_log
- cursor
- account_id
- note_id
- op_type
- note_version
- payload_json
- device_id
- created_at
```

`change_log.cursor` is a monotonically increasing account-visible event position. Clients use it to pull incremental changes.

## Configuration

The sync server supports these required environment variables:

- `DATABASE_URL`
- `JWT_SECRET`
- `ASSET_DATA_DIR`
- `PUBLIC_BASE_URL`
- `ALLOW_REGISTRATION`
- `SNAPLINE_BOOTSTRAP_ADMIN_EMAIL`
- `SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD`

`ALLOW_REGISTRATION=true` permits public account creation. `ALLOW_REGISTRATION=false` disables open registration for self-hosted deployments that want admin-created accounts only.

When registration is disabled and no account exists, the server creates the first account from `SNAPLINE_BOOTSTRAP_ADMIN_EMAIL` and `SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD` on startup. After at least one account exists, those variables are ignored. M2 does not include a full admin panel.

## API

Authentication:

```text
POST /auth/register
POST /auth/login
```

Sync:

```text
POST /sync/push
GET  /sync/pull?cursor=<cursor>
GET  /sync/snapshot
```

Assets:

```text
POST /sync/assets/upload
GET  /sync/assets/:asset_id/download
```

All sync and asset endpoints require authentication. Requests include the client `device_id`.

## Push Behavior

The client sends queued note changes with the note's `base_version`.

If the server note version equals `base_version`, the server accepts the change:

1. Update the `notes` row.
2. Increment the note version.
3. Append a `change_log` event.
4. Return the new version and cursor.

If the server note version does not equal `base_version`, the server rejects that note change as a conflict and returns the current server note state.

Asset upload runs before note push when a note references a locally missing remote asset. This ensures other devices can render the note after pulling it.

## Pull Behavior

The client calls `pull` with its last known cursor. The server returns changes after that cursor for the authenticated account.

When applying remote changes:

- If the local note has no unsynced changes, apply the remote state.
- If the remote change originated from this device, mark the matching queued item as synced and update `server_version`.
- If the local note has unsynced edits and the remote version advanced, preserve the local edit as a conflict copy.

Asset downloads are lazy. After applying a pulled note, the client scans Markdown image references. Missing local assets are queued for background download.

## Snapshot Behavior

`snapshot` returns the current server state for an account:

- notes
- asset metadata
- latest cursor

It is used for first sync on a new device and recovery if incremental sync state is invalid.

Snapshot does not need to include asset bytes. The client downloads missing assets separately.

## Conflict Handling

M2 does not merge Markdown bodies. Version conflicts create explicit conflict copies.

When upload is rejected:

1. Keep the server version as the canonical original note.
2. Create a local conflict copy with the rejected local content.
3. Set `is_conflict_copy = 1`.
4. Set `source_note_id` to the original note id.
5. Keep Markdown image references unchanged.
6. Do not delete related assets.

Conflict copies are local notes that can later sync as independent notes if the user keeps editing them.

Image assets are deduplicated by `asset_id` and `sha256`. If two conflicting note versions reference different assets, both assets remain available.

## Client UX

M2 UI should stay quiet:

- A sync status indicator: `Synced`, `Syncing`, `Offline`, `Error`, `Conflict`.
- Login and sync server URL settings.
- Clear labeling for conflict copies.
- Retry behavior that does not require user intervention for transient failures.

The editor remains usable while logged out or offline.

## Deployment

Self-hosted M2 deployment should include Docker Compose for:

- `snapline-sync-server`
- `postgres`
- persistent PostgreSQL volume
- persistent asset data volume

The self-hosting guide must explain backup as two parts:

- PostgreSQL database
- `ASSET_DATA_DIR`

## Testing

M2 requires test coverage for:

- Queue creation for save, pin, delete, and asset upload.
- Idempotent asset upload.
- Push accept path.
- Push conflict path.
- Pull incremental changes.
- Snapshot import.
- Lazy asset download queueing.
- Conflict copy creation with image references preserved.
- `ALLOW_REGISTRATION=false` rejecting public registration.

Manual multi-device verification:

1. Device A creates a note; Device B receives it.
2. Device A edits a note; Device B receives the edit.
3. Device A deletes a note; Device B reflects deletion.
4. Device A pins a note; Device B reflects pinned ordering.
5. Device A pastes an image; Device B downloads and displays it.
6. Device A and B edit the same note offline; reconnecting creates a conflict copy.
7. Server restarts without data loss when PostgreSQL and asset volumes persist.

## Milestones

### M2A: Local Sync Foundation

- Add sync fields to `notes`.
- Add `change_queue`.
- Add `sync_state`.
- Generate queued changes for note save, title update, pin, delete, and image paste.
- Add mock sync service for queue and conflict tests.

### M2B: Open Source Sync Server

- Add `crates/sync-server`.
- Add Axum HTTP service.
- Add PostgreSQL migrations.
- Add register and login.
- Add push, pull, snapshot.
- Add asset upload and download.
- Add `LocalFsAssetStore`.

### M2C: Real Client Sync

- Add `crates/sync-client`.
- Add login settings.
- Add custom server URL setting.
- Run background push and pull.
- Add retry and sync status.
- Add first-login snapshot flow.

### M2D: Multi-Device Validation

- Verify note creation, edit, delete, pin, and image sync.
- Verify offline edits and conflict copies.
- Verify Docker Compose self-hosting.
- Write self-hosting and backup documentation.

## Open Decisions Deferred

- S3 and MinIO implementation.
- Password reset and email verification.
- Official hosted service rate limiting and abuse prevention.
- Secure token storage fallback details.
- Rich admin account management.
