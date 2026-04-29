# Snapline Self-Hosting

Snapline sync server is an open source Axum service backed by PostgreSQL. M2 stores image assets on the server filesystem.

## Start Locally

```powershell
docker compose -f docker-compose.sync.yml up --build
```

The server listens on `http://localhost:8080`.

## Required Configuration

- `DATABASE_URL`: PostgreSQL connection string.
- `JWT_SECRET`: long random secret for access tokens.
- `ASSET_DATA_DIR`: directory for image assets.
- `PUBLIC_BASE_URL`: external URL clients use.
- `ALLOW_REGISTRATION`: `true` or `false`.
- `SNAPLINE_BOOTSTRAP_ADMIN_EMAIL`: first account email when registration is disabled.
- `SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD`: first account password when registration is disabled.

## Backup

Back up both PostgreSQL and the asset directory.

For Docker Compose deployments, preserve:

- `snapline-postgres`
- `snapline-assets`

## Disable Public Registration

Set:

```env
ALLOW_REGISTRATION=false
SNAPLINE_BOOTSTRAP_ADMIN_EMAIL=you@example.com
SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD=a-long-password
```

The bootstrap account is created only when no account exists.
