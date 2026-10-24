#!/usr/bin/env bash
set -euo pipefail

# Question 3 of the notifications-takeover spike: the login ordering, checked on copies of the units in a temp dir
# with the offline `systemd-analyze --user verify`. It never talks to the user's systemd manager: no systemctl, no
# daemon-reload, and SYSTEMD_UNIT_PATH holds only the temp dir and the packaged units under /usr/lib/systemd/user.

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
packaged=/usr/lib/systemd/user
tmp=$(mktemp -d -t 3b-t1-units.XXXXXX)
trap 'rm -r -- "$tmp"' EXIT

export SYSTEMD_UNIT_PATH="$tmp:$packaged"
analyze=(systemd-analyze --user --no-pager)

cp -- "$repo/packaging/systemd/ghostd.service" "$tmp/"
cp -- "$packaged/plasma-plasmashell.service" "$tmp/"
sed -i 's/^Restart=on-failure$/Type=notify\nRestart=on-failure/' "$tmp/ghostd.service"
mkdir "$tmp/plasma-plasmashell.service.d"
cat > "$tmp/plasma-plasmashell.service.d/ghost-shell.conf" <<- 'DROPIN'
	[Unit]
	After=ghostd.service
	Wants=ghostd.service
DROPIN

echo "== plasma-plasmashell.service and its drop-in"
cat "$tmp/plasma-plasmashell.service" "$tmp"/plasma-plasmashell.service.d/ghost-shell.conf | grep -v "^$"
echo "== ghostd.service with Type=notify"
grep -v "^$" "$tmp/ghostd.service"
echo "== verify, both units: exit $("${analyze[@]}" verify "$tmp/ghostd.service" "$tmp/plasma-plasmashell.service" > "$tmp/verify.out" 2>&1 && echo 0 || echo $?)"
cat "$tmp/verify.out"

# systemd-analyze cat-config has no --user and systemctl --user show would need the live manager, so prove the After= edge is live offline: a unit ordered
# the other way must make verify report an ordering cycle.
cp -- "$tmp/ghostd.service" "$tmp/ghostd-cycle.service"
sed -i 's/^PartOf=graphical-session.target$/PartOf=graphical-session.target\nAfter=plasma-plasmashell.service/' "$tmp/ghostd-cycle.service"
sed -i 's/ghostd.service/ghostd-cycle.service/' "$tmp/plasma-plasmashell.service.d/ghost-shell.conf"
echo "== control: ghostd ordered after plasmashell too, exit $("${analyze[@]}" verify "$tmp/ghostd-cycle.service" "$tmp/plasma-plasmashell.service" > "$tmp/cycle.out" 2>&1 && echo 0 || echo $?)"
cat "$tmp/cycle.out"
