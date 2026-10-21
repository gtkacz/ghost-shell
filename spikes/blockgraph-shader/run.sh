#!/usr/bin/env bash
set -euo pipefail

# Measures the spike's modes against one fake ghostd. Everything runs on a private bus made for this
# run, and every process is stopped by the pid it was started with.
#   SPIKE_OUT        directory for logs, grabs and results (default: a new temp dir)
#   SPIKE_STAGES     any of "compare measure" (default: both)
#   SPIKE_WARMUP_S   seconds the daemon records history before the first stage
#   SPIKE_REPEATS    measured runs per mode at the default cell
#   SPIKE_MEASURE_S  seconds of each CPU measurement
#   SPIKE_TICK_MS    the daemon tick, for a quick check of the script only (the history then fills faster)
#   SPIKE_PID_FILE   where the inner script writes its pid, so a stop can reach it

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)

if [[ -z ${SPIKE_INNER:-} ]]; then
    exec env SPIKE_INNER=1 dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- "${BASH_SOURCE[0]}" "$@"
fi
if [[ -n ${SPIKE_PID_FILE:-} ]]; then
    echo "$$" > "$SPIKE_PID_FILE"
fi

out=${SPIKE_OUT:-$(mktemp -d -t blockgraph-spike.XXXXXX)}
stages=${SPIKE_STAGES:-compare measure}
# The shell keeps five minutes of history, 300 samples, so after this every lane starts with all of it
# (a 4 px cell lane shows 251 columns; a 3 px one, 314, the oldest 14 of them missing).
warmup_s=${SPIKE_WARMUP_S:-320}
repeats=${SPIKE_REPEATS:-3}
measure_s=${SPIKE_MEASURE_S:-60}
settle_s=10
created_deadline_s=30
socket_deadline_s=10
grab_deadline_s=20
poll_s=0.2

daemon=$repo/target/release/ghostd
socket=$out/ghostd.sock
config=$out/config.toml
results=$out/results.txt
frame_line='syncAndRender: frame rendered'

daemon_pid=
warm_pid=
qs_pid=

stop() {
    local -n pid_var=$1
    if [[ -n $pid_var ]]; then
        kill "$pid_var" 2> /dev/null || true
        wait "$pid_var" 2> /dev/null || true
        pid_var=
    fi
}

cleanup() {
    exec 3>&- 2> /dev/null || true
    stop qs_pid
    stop warm_pid
    stop daemon_pid
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

wait_for() {
    local deadline=$((SECONDS + $1))
    shift
    while (( SECONDS < deadline )); do
        if "$@"; then
            return 0
        fi
        sleep "$poll_s"
    done
    return 1
}

# grep -c prints 0 and exits 1 when nothing matches, which is a valid count here.
count_in() {
    grep -ac -- "$1" "$2" || true
}

has_at_least() {
    (( $(count_in "$1" "$2") >= $3 ))
}

start_qs() {
    local mode=$1 cell=$2 log=$3
    env SPIKE_MODE="$mode" SPIKE_CELL="$cell" SPIKE_GRAB_DIR="$out" GHOST_SOCKET="$socket" \
        QML_IMPORT_PATH="$repo/shell/kit" QSG_RENDER_TIMING=1 qs --no-color -p "$spike" > "$log" 2>&1 &
    qs_pid=$!
}

# How many pixels of two grabs differ at all, and the largest difference in any channel, from 0 to 1.
diff_pair() {
    local differing peak
    # compare exits 1 when the images differ, which is a result here, not a failure.
    differing=$(magick compare -metric AE "$1" "$2" null: 2>&1 || true)
    peak=$(magick compare -metric PAE "$1" "$2" null: 2>&1 || true)
    echo "differing_pixels=${differing%% *} peak_error=${peak##* }"
}

compare() {
    local cell=$1 log=$out/compare-$1.log
    local grabs=(rect shader rect-live1 shader-live1 rect-live2 shader-live2)
    start_qs compare "$cell" "$log"
    if ! wait_for "$grab_deadline_s" has_at_least "spike grab saved" "$log" "${#grabs[@]}"; then
        echo "compare $cell: grabs missing, see $log" | tee -a "$results"
        stop qs_pid
        return
    fi
    stop qs_pid
    local grab
    for grab in "${grabs[@]}"; do
        mv -- "$out/$grab.png" "$out/$grab-$cell.png"
    done
    local at="compare cell=$cell size=$(magick identify -format '%wx%h' "$out/rect-$cell.png")"
    echo "$at fixed: $(diff_pair "$out/rect-$cell.png" "$out/shader-$cell.png")" | tee -a "$results"
    echo "$at live1: $(diff_pair "$out/rect-live1-$cell.png" "$out/shader-live1-$cell.png")" | tee -a "$results"
    echo "$at live2: $(diff_pair "$out/rect-live2-$cell.png" "$out/shader-live2-$cell.png")" | tee -a "$results"
    echo "$at shader live1 vs live2, which must differ: $(diff_pair "$out/shader-live1-$cell.png" "$out/shader-live2-$cell.png")" | tee -a "$results"
}

measure() {
    local mode=$1 cell=$2 label=$3 log=$out/$3.log
    start_qs "$mode" "$cell" "$log"
    if ! wait_for "$created_deadline_s" has_at_least "spike lanes created" "$log" 1; then
        echo "$label: lanes never created, see $log" | tee -a "$results"
        stop qs_pid
        return
    fi
    local rss_created
    rss_created=$(awk '/^VmRSS:/ { print $2 }' "/proc/$qs_pid/status")
    sleep "$settle_s"
    local frames_before frames_after usage
    frames_before=$(count_in "$frame_line" "$log")
    usage=$("$repo/scripts/measure-cpu.sh" "$qs_pid" "$measure_s")
    frames_after=$(count_in "$frame_line" "$log")
    stop qs_pid
    local created first_frame frames frames_per_s
    created=$(grep -aoE 'lanes created in [0-9]+ ms' "$log" | grep -oE '[0-9]+')
    # The first frame logged after the lanes exist is the one that builds their scene-graph nodes.
    first_frame=$(awk -v line="$frame_line" \
        '/spike lanes created/ { after = 1 } after && index($0, line) { sub(/.*frame rendered in /, ""); print; exit }' "$log")
    # The log reaches the file late: runs whose frames were all 1 s apart counted as few as 56 in 60 s.
    frames=$((frames_after - frames_before))
    frames_per_s=$(awk -v f="$frames" -v s="$measure_s" 'BEGIN { printf "%.2f", f / s }')
    echo "$label mode=$mode cell=$cell $usage rss_at_creation_kib=$rss_created frames=$frames" \
        "frames_per_s=$frames_per_s create_ms=$created first_frame=[$first_frame]" | tee -a "$results"
}

qsb-qt6 --qt6 -o "$spike/blockgraph.frag.qsb" "$spike/blockgraph.frag"
"$daemon" default-config > "$config"
: > "$results"
echo "out=$out" | tee -a "$results"

tick_args=()
if [[ -n ${SPIKE_TICK_MS:-} ]]; then
    tick_args=(--tick-ms "$SPIKE_TICK_MS")
fi
# On this private bus only, so its notifications reach nobody.
"$daemon" run --fake --socket "$socket" --config "$config" --widgets-dir "$repo/widgets" "${tick_args[@]}" > "$out/ghostd.log" 2>&1 &
daemon_pid=$!
wait_for "$socket_deadline_s" test -S "$socket"

# A client that keeps cpu and gpu subscribed between the measured runs, so the daemon keeps recording
# history and each run starts with all of it.
mkfifo "$out/warm.in"
socat UNIX-CONNECT:"$socket" - < "$out/warm.in" > /dev/null &
warm_pid=$!
exec 3> "$out/warm.in"
printf '%s\n' '{"t":"hello","proto":1,"topics":[]}' '{"t":"subscribe","topics":["cpu","gpu"]}' >&3

sleep "$warmup_s"
for stage in $stages; do
    case $stage in
    compare)
        compare 4x3
        compare 3x2
        ;;
    measure)
        for run in $(seq 1 "$repeats"); do
            measure none 4x3 "none-$run"
            measure rect 4x3 "rect-$run"
            measure shader 4x3 "shader-$run"
        done
        measure rect 3x2 rect-3x2
        measure shader 3x2 shader-3x2
        ;;
    *)
        echo "unknown stage $stage" >&2
        exit 64
        ;;
    esac
done
