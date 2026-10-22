# Sourced by qml-smoke.sh and smoke-holds.sh: which lines of the shell's log are errors, so the two
# scripts fail on the same lines. Requires bash.

# "ghostd refused the shell" catches the daemon refusing the shell, which Data logs at error level and
# which would otherwise only show up as a reconnect loop. "ghostd config problem" catches a layout
# naming a widget that is missing or unusable, which the daemon reports instead of refusing. qs's
# "Failed to connect pipewire context. Errno: 112", which every run with PipeWire shut out logs, is no
# error: "Errno:" is not "Error:".
shell_error_pattern='ReferenceError|TypeError|is not defined|Cannot assign|Error:|failed to load|Unable to|ghostd refused the shell|ghostd config problem'

# The two test fixtures exist to show that a broken widget and a missing one are reported, so their
# lines never count as errors, and a layout that names a fixture must produce its line.
declare -A fixture_lines=(
    [broken-example]='widget "broken-example" failed to load'
    [not-installed]='no widget named "not-installed" is installed'
)

# Extra shell-log lines to accept, as an extended regex, e.g. for widgets a layout names before they
# exist.
extra_allowed=${GHOST_SMOKE_ALLOW:-}

# Prints why GHOST_SMOKE_ALLOW cannot be used and returns 1, or returns 0. A pattern grep cannot
# compile (exit 2) makes the final filter drop every line, and so does one that matches an empty line
# (exit 0), such as `.*` or `foo|`: either way the check would pass with all its errors hidden.
check_extra_allowed() {
    local allow_status=0
    if [[ -z $extra_allowed ]]; then
        return 0
    fi
    grep -qE -- "$extra_allowed" <<< "" || allow_status=$?
    if (( allow_status == 2 )); then
        echo "GHOST_SMOKE_ALLOW is not a valid extended regex: $extra_allowed"
        return 1
    fi
    if (( allow_status == 0 )); then
        echo "GHOST_SMOKE_ALLOW matches an empty line, so it would hide every error: $extra_allowed"
        return 1
    fi
}

# The lines a log may hold that match the error pattern: the fixtures', GHOST_SMOKE_ALLOW's and any
# given as arguments, each an extended regex.
allowed_pattern() {
    local pattern extra
    pattern=$(IFS='|'; echo "${fixture_lines[*]}")
    if [[ -n $extra_allowed ]]; then
        pattern+="|$extra_allowed"
    fi
    for extra in "$@"; do
        pattern+="|$extra"
    done
    echo "$pattern"
}

# Prints the error lines of log $1 that no allowed pattern covers; extra allowed patterns follow it.
# grep exits 1 when nothing is left, which is the passing case here.
shell_log_errors() {
    local log=$1
    shift
    grep -aE -- "$shell_error_pattern" "$log" | grep -avE -- "$(allowed_pattern "$@")" || true
}

# Prints how many lines of log $1 are errors that an allowed pattern covers; extra allowed patterns
# follow it.
shell_log_allowed() {
    local log=$1
    shift
    grep -aE -- "$shell_error_pattern" "$log" | grep -acE -- "$(allowed_pattern "$@")" || true
}

# Prints the line of each fixture config $1 names that log $2 lacks.
missing_fixture_lines() {
    local fixture
    for fixture in "${!fixture_lines[@]}"; do
        if grep -qE "widget *= *\"$fixture\"" "$1" && ! grep -qF -- "${fixture_lines[$fixture]}" "$2"; then
            echo "${fixture_lines[$fixture]}"
        fi
    done
}
