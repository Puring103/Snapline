#!/bin/sh
set -eu

root=/opt/snapline
stamp=$(date -u +%Y%m%dT%H%M%SZ)
destination="$root/backups/$stamp"
mkdir -p "$destination"

cd "$root/current"
docker compose --env-file "$root/.env" -f deploy/compose.yml exec -T postgres \
  pg_dump -U snapline -d snapline -Fc > "$destination/postgres.dump"
docker run --rm -v snapline_object-data:/source:ro -v "$destination:/backup" alpine:3.22 \
  tar -czf /backup/objects.tar.gz -C /source .
sha256sum "$destination/postgres.dump" "$destination/objects.tar.gz" > "$destination/SHA256SUMS"
echo "$destination"
