# shellcheck shell=bash
# Shared by plasma-panel.sh and plasma-notifications.sh, which source it: paths, the commands that stop and start
# plasmashell, step/warn/die, the recovery trap and the KGlobalAccel and config reads both subcommand families use.
# GHOST_PLASMA_STOP, GHOST_PLASMA_START and GHOST_PLASMA_CHECK each name one executable that replaces the command that
# stops plasmashell, starts it, or succeeds when plasmashell runs under its systemd user unit (defaults: that unit's
# systemctl calls).

config_home=${XDG_CONFIG_HOME:-$HOME/.config}
state_home=${XDG_STATE_HOME:-$HOME/.local/state}
state_dir=$state_home/ghost-shell
backup_root=$state_dir/plasma-backup
retired_marker=$state_dir/plasma-retired

plasma_unit=plasma-plasmashell.service
plasma_stop=(systemctl --user stop "$plasma_unit")
plasma_start=(systemctl --user start "$plasma_unit")
plasma_check=(unit_owns_plasmashell)
# One word each, so a path with spaces reaches the command whole.
if [[ -n ${GHOST_PLASMA_STOP:-} ]]; then plasma_stop=("$GHOST_PLASMA_STOP"); fi
if [[ -n ${GHOST_PLASMA_START:-} ]]; then plasma_start=("$GHOST_PLASMA_START"); fi
if [[ -n ${GHOST_PLASMA_CHECK:-} ]]; then plasma_check=("$GHOST_PLASMA_CHECK"); fi
plasma_deadline_s=60
poll_s=0.2

required_tools=(busctl jq kwriteconfig6)
# A Qt key sequence holds four keys; KGlobalAccel's own client always sends all four.
keys_per_sequence=4
# Positions in a KGlobalShortcutInfo as allShortcutInfos returns it.
info_action_name_at=0
info_component_at=2
info_keys_at=6
# Where ghost-shell keeps its settings; absent keys mean ghostd's defaults.
ghost_config=$config_home/ghost-shell/config.toml

# Set once a change has been made that only restore can undo, and while this script holds plasmashell stopped.
recovery_armed=0
plasma_stopped=0
failures=()
# What recover reports: the marker that names the backup, and the subcommand that undoes the change under way.
recovery_marker=$retired_marker
recovery_command=restore

step() {
    printf '==> %s\n' "$*"
}

warn() {
    printf 'warning: %s\n' "$*" >&2
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

on_error() {
    local status=$?
    printf 'error: "%s" failed with status %d\n' "$BASH_COMMAND" "$status" >&2
    return "$status"
}

# The one place every way out passes (ERR and die exit, and INT, HUP and TERM are turned into exits), so a failure after
# the first change never leaves plasmashell stopped, and always says how to recover.
recover() {
    local status=$?
    trap - EXIT ERR
    if ((status == 0 || ! recovery_armed)); then
        exit "$status"
    fi
    if [[ -e $recovery_marker ]]; then
        printf 'error: the backup is in %s; recover with: %q %s\n' \
            "$backup_root/$(< "$recovery_marker")" "$0" "$recovery_command" >&2
    fi
    if ((plasma_stopped)); then
        printf 'error: plasmashell was stopped; starting it again so the desktop is not left blank\n' >&2
        start_plasmashell || warn "plasmashell did not start; start it with: ${plasma_start[*]@Q}"
    fi
    exit "$status"
}
trap recover EXIT
trap 'exit 130' INT
trap 'exit 129' HUP
trap 'exit 143' TERM
trap on_error ERR

check_paths() {
    # Relative dirs would put the backup and the popup map wherever the script happens to run.
    if [[ $config_home != /* || $state_home != /* ]]; then
        die "XDG_CONFIG_HOME ($config_home) and XDG_STATE_HOME ($state_home) must be absolute"
    fi
}

check_prerequisites() {
    local tool
    : "${DBUS_SESSION_BUS_ADDRESS:?DBUS_SESSION_BUS_ADDRESS must be set to the session bus Plasma runs on}"
    for tool in "${required_tools[@]}"; do
        if ! command -v "$tool" > /dev/null 2>&1; then
            die "$tool not found in PATH"
        fi
    done
}

session_call() {
    busctl --user "$@"
}

name_owned() {
    [[ $(session_call call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus NameHasOwner s "$1") == "b true" ]]
}

# The script restarts plasmashell through its unit; if the unit is not what runs it, stopping the unit stops nothing.
unit_owns_plasmashell() {
    local cgroup owner
    systemctl --user is-active --quiet "$plasma_unit" || return 1
    cgroup=$(systemctl --user show --property ControlGroup --value "$plasma_unit") || return 1
    owner=$(session_call call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus \
        GetConnectionUnixProcessID s org.kde.plasmashell) || return 1
    [[ -n $cgroup && $owner == u\ * && -r /proc/${owner#u }/cgroup ]] || return 1
    [[ $(< "/proc/${owner#u }/cgroup") == *"$cgroup"* ]]
}

wait_for_plasmashell() {
    local want=$1 deadline=$((SECONDS + plasma_deadline_s))
    while ((SECONDS < deadline)); do
        if { [[ $want == up ]] && name_owned org.kde.plasmashell; } || { [[ $want == down ]] && ! name_owned org.kde.plasmashell; }; then
            return 0
        fi
        sleep "$poll_s"
    done
    warn "plasmashell is not $want after ${plasma_deadline_s} s"
    return 1
}

stop_plasmashell() {
    step "stopping plasmashell: ${plasma_stop[*]@Q}"
    # Set first: a stop that fails halfway may still have taken plasmashell down.
    plasma_stopped=1
    "${plasma_stop[@]}" && wait_for_plasmashell down
}

start_plasmashell() {
    step "starting plasmashell: ${plasma_start[*]@Q}"
    "${plasma_start[@]}" && wait_for_plasmashell up && plasma_stopped=0
}

# Prints "<component>/<action>" for each action that holds the key, an int as KGlobalAccel stores it.
key_holders() {
    local reply
    reply=$(session_call --json=short call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel \
        globalShortcutsByKey '(ai)(i)' "$keys_per_sequence" "$1" 0 0 0 0) || return 1
    jq -r --argjson component_at "$info_component_at" --argjson action_at "$info_action_name_at" \
        '.data[0][] | .[$component_at] + "/" + .[$action_at]' <<< "$reply"
}

# Prints the value of <key> in the TOML table <section> of ghost-shell's config, or nothing. Handles the plain
# key = "value" lines ghostd writes, not inline tables.
config_value() {
    [[ -f $ghost_config ]] || return 0
    awk -v section="$1" -v key="$2" '
        /^[[:space:]]*\[/ { current = $0; gsub(/^[[:space:]]*\[|\][[:space:]]*(#.*)?$/, "", current); next }
        current == section && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
            value = $0
            sub(/^[^=]*=[[:space:]]*/, "", value)
            sub(/[[:space:]]+#.*$/, "", value)
            gsub(/^["\x27]|["\x27]$/, "", value)
            print value
            exit
        }' "$ghost_config"
}

# Runs one command and keeps going if it fails, so restore does everything it can and reports what it could not.
attempt() {
    if ! "$@"; then
        failures+=("$*")
        warn "failed: $*"
    fi
}
