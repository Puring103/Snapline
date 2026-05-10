# Snapline

Snapline is a desktop-first note application built with Tauri, React, TypeScript, and Rust. It stores notes locally, supports embedded image assets, and includes an optional self-hosted sync stack for multi-device use.

## Repository Map

- `apps/client/`
  Cross-platform Tauri client UI and native integration for desktop and mobile.
- `crates/domain/`
  Shared domain types for notes, assets, and sync payloads.
- `crates/platform/`
  Platform path helpers and desktop environment support code.
- `crates/storage/`
  Local persistence and sync queue state.
- `crates/app-core/`
  Application use cases that coordinate storage, assets, and sync.
- `crates/sync-client/`
  Client-side sync protocol and processing logic.
- `crates/sync-server/`
  Axum-based sync server for self-hosted deployments.
- `docs/`
  Linux support, self-hosting notes, and project design history.

## Workspace Architecture

The cross-platform client lives in `apps/client`, with React and Tauri on the frontend side and Rust crates in the workspace providing storage, business logic, and sync support.

The sync stack is split across:

- `crates/sync-client/` for client-side queue processing and remote sync calls
- `crates/sync-server/` for the self-hosted API, auth, and asset persistence

## Desktop Development

Install frontend dependencies:

```bash
cd apps/client
npm install
```

Run the desktop frontend dev server:

```bash
cd apps/client
npm run dev
```

Run the Tauri desktop app:

```bash
cd apps/client
npx tauri dev
```

Create a production frontend build:

```bash
cd apps/client
npm run build
```

## Testing

Run the frontend test suite:

```bash
cd apps/client
npm test
```

Run the Rust workspace tests:

```bash
cargo test
```

Run targeted crate tests while refactoring:

```bash
cargo test -p snapline-app-core
cargo test -p snapline-storage
```

## Sync Server

Start the self-hosted sync stack locally with Docker Compose:

```bash
docker compose -f docker-compose.sync.yml up --build
```

The default local server endpoint is `http://localhost:8080`.

## Platform Notes

- Linux build and runtime notes: [docs/linux.md](docs/linux.md)
- Self-hosting and deployment notes: [docs/self-hosting.md](docs/self-hosting.md)

## Project History

The repository also keeps internal design and implementation history:

- `docs/superpowers/specs/`
- `docs/superpowers/plans/`

These are useful for engineering context, but `README.md` plus the operational docs above should be the primary entry points for day-to-day development.
