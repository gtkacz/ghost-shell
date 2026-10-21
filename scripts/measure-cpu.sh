#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: ${0##*/} <pid> <seconds>" >&2
    exit 64
fi

pid=$1
seconds=$2

if [[ ! -r /proc/$pid/stat ]]; then
    echo "no such process $pid" >&2
    exit 2
fi

clk_tck=$(getconf CLK_TCK)

# The comm field may itself contain spaces and parentheses, so split only what follows the last ")".
cpu_ticks() {
    local stat fields
    { stat=$(< "/proc/$pid/stat"); } 2>/dev/null || return 1
    read -ra fields <<< "${stat##*) }"
    # After the comm field, state is index 0, so utime (field 14) and stime (field 15) are 11 and 12.
    echo $(( fields[11] + fields[12] ))
}

start_ticks=$(cpu_ticks) || { echo "no such process $pid" >&2; exit 2; }
start_time=$EPOCHREALTIME
sleep "$seconds"
end_ticks=$(cpu_ticks) || { echo "process $pid exited during measurement" >&2; exit 3; }
end_time=$EPOCHREALTIME

rss_kib=$(awk '/^VmRSS:/ { print $2 }' "/proc/$pid/status")

# Divide by the measured elapsed time, not the requested one, so scheduler jitter in sleep does not skew the result.
awk -v ticks=$(( end_ticks - start_ticks )) -v tck="$clk_tck" -v t0="$start_time" -v t1="$end_time" -v rss="$rss_kib" \
    'BEGIN { printf "cpu_pct=%.2f rss_kib=%d\n", 100 * ticks / tck / (t1 - t0), rss }'
