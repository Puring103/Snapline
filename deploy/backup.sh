#!/bin/sh
set -eu

root=${SNAPLINE_ROOT:-/opt/snapline}
project=${SNAPLINE_COMPOSE_PROJECT:-snapline}
case "$project" in *[!A-Za-z0-9_-]*) echo "invalid compose project" >&2; exit 2 ;; esac
stamp=$(date -u +%Y%m%dT%H%M%SZ)
destination="$root/backups/$stamp"
mkdir -p "$destination"

cd "$root/current"
docker compose -p "$project" --env-file "$root/.env" -f deploy/compose.yml exec -T postgres \
  pg_dump -U snapline -d snapline -Fc > "$destination/postgres.dump"
docker run --rm -v "${project}_object-data:/source:ro" -v "$destination:/backup" alpine:3.22 \
  tar -czf /backup/objects.tar.gz -C /source .
cd "$destination"
sha256sum postgres.dump objects.tar.gz > SHA256SUMS
echo "$destination"
