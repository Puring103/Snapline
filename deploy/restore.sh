#!/bin/sh
set -eu

root=${SNAPLINE_ROOT:-/opt/snapline}
project=${SNAPLINE_COMPOSE_PROJECT:-snapline}
health_url=${SNAPLINE_HEALTH_URL:-http://127.0.0.1/snapline/health/ready}
case "$project" in *[!A-Za-z0-9_-]*) echo "invalid compose project" >&2; exit 2 ;; esac
stamp=${1:-}
case "$stamp" in
  [0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]T[0-9][0-9][0-9][0-9][0-9][0-9]Z) ;;
  *) echo "invalid backup timestamp" >&2; exit 2 ;;
esac
backup="$root/backups/$stamp"
[ -d "$backup" ] || { echo "backup not found" >&2; exit 2; }
cd "$backup"
sha256sum -c SHA256SUMS
[ "$(readlink -f "$backup")" = "$backup" ] || { echo "backup path is not canonical" >&2; exit 2; }

cd "$root/current"
docker compose -p "$project" --env-file "$root/.env" -f deploy/compose.yml stop api
docker compose -p "$project" --env-file "$root/.env" -f deploy/compose.yml exec -T postgres dropdb -U snapline --if-exists snapline
docker compose -p "$project" --env-file "$root/.env" -f deploy/compose.yml exec -T postgres createdb -U snapline snapline
docker compose -p "$project" --env-file "$root/.env" -f deploy/compose.yml exec -T postgres pg_restore -U snapline -d snapline --no-owner --no-privileges < "$backup/postgres.dump"
docker run --rm -v "${project}_object-data:/target" -v "$backup:/backup:ro" alpine:3.22 sh -c 'find /target -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +; tar -xzf /backup/objects.tar.gz -C /target; chown -R 10001:10001 /target'
docker compose -p "$project" --env-file "$root/.env" -f deploy/compose.yml up -d api
for attempt in $(seq 1 30); do
  if curl --fail --silent "$health_url" >/dev/null; then
    echo "restored $backup"
    exit 0
  fi
  sleep 2
done
echo "restore completed but health check failed" >&2
exit 1
