#!/usr/bin/env bash
# Downloads DB-IP's free City Lite and ASN Lite databases (CC BY 4.0) for the uplink globe.
# Run it by hand: it reaches download.db-ip.com, so install.sh never runs it. DB-IP publishes a
# month's files during that month, so early in a month the previous one may be the latest.
set -euo pipefail

: "${HOME:?HOME must be set}"

dest=${XDG_DATA_HOME:-$HOME/.local/share}/ghost-shell/geoip
base_url=https://download.db-ip.com/free
editions=(city asn)
http_ok=200
http_not_found=404

this_month=$(date -u +%Y-%m)
last_month=$(date -u -d "$(date -u +%Y-%m-01) -1 month" +%Y-%m)

mkdir -p -- "$dest"
# Temp files live beside the databases, so the final mv is a rename that never leaves a
# half-written file where ghostd maps it.
scratch=$(mktemp -d -- "$dest/.fetch.XXXXXX")
trap 'rm -r -- "$scratch"' EXIT

# Downloads `dbip-<edition>-lite-<month>.mmdb.gz` to $2; prints the HTTP status.
download() {
    local edition=$1 month=$2 out=$3
    curl --silent --show-error --location --output "$out" --write-out '%{http_code}' \
        "$base_url/dbip-$edition-lite-$month.mmdb.gz"
}

for edition in "${editions[@]}"; do
    gz=$scratch/$edition.mmdb.gz
    status=$(download "$edition" "$this_month" "$gz")
    if [[ $status == "$http_not_found" ]]; then
        status=$(download "$edition" "$last_month" "$gz")
    fi
    if [[ $status != "$http_ok" ]]; then
        echo "fetch-geoip: dbip-$edition-lite: HTTP $status" >&2
        exit 1
    fi
    gunzip --stdout -- "$gz" > "$scratch/$edition.mmdb"
    mv -- "$scratch/$edition.mmdb" "$dest/dbip-$edition-lite.mmdb"
done

du -h -- "$dest"/dbip-*-lite.mmdb
