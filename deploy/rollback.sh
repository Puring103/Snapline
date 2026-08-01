#!/bin/sh
set -eu

root=${SNAPLINE_ROOT:-/opt/snapline}
project=${SNAPLINE_COMPOSE_PROJECT:-snapline}
health_url=${SNAPLINE_HEALTH_URL:-http://127.0.0.1/snapline/health/ready}
case "$project" in *[!A-Za-z0-9_-]*) echo "invalid compose project" >&2; exit 2 ;; esac
requested=${1:-previous}
current=$(readlink -f "$root/current")
case "$requested" in
  previous)
    target=$(find "$root/releases" -mindepth 1 -maxdepth 1 -type d ! -path "$current" -printf '%p\n' | sort -r | head -n 1)
    ;;
  [0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]T[0-9][0-9][0-9][0-9][0-9][0-9]Z)
    target="$root/releases/$requested"
    ;;
  *) echo "invalid release" >&2; exit 2 ;;
esac
[ -n "$target" ] && [ -d "$target" ] || { echo "release not found" >&2; exit 2; }
case "$(readlink -f "$target")" in "$root/releases/"*) ;; *) echo "release path escaped root" >&2; exit 2 ;; esac

sh "$current/deploy/backup.sh"
cd "$target"
docker compose -p "$project" --env-file "$root/.env" -f deploy/compose.yml build api
ln -sfn "$target" "$root/current"
docker compose -p "$project" --env-file "$root/.env" -f "$root/current/deploy/compose.yml" up -d --force-recreate
if ! sh -c 'for attempt in $(seq 1 30); do curl --fail --silent "$1" >/dev/null && exit 0; sleep 2; done; exit 1' sh "$health_url"; then
  ln -sfn "$current" "$root/current"
  docker compose -p "$project" --env-file "$root/.env" -f "$root/current/deploy/compose.yml" up -d --force-recreate
  echo "rollback target failed health check; previous release restored" >&2
  exit 1
fi
echo "active release: $target"
