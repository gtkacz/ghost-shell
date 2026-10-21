#!/usr/bin/env bash
set -euo pipefail

: "${HOME:?HOME must be set}"

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
data_home=${XDG_DATA_HOME:-$HOME/.local/share}
config_home=${XDG_CONFIG_HOME:-$HOME/.config}

bin_dir=$HOME/.local/bin
# Pinned to $HOME/.local/share rather than $XDG_DATA_HOME because the units hard-code
# %h/.local/share and ghostd looks for its built-in widgets there; honouring the variable would
# install the shell and widgets where neither is looking.
app_dir=$HOME/.local/share/ghost-shell
shell_dir=$app_dir/shell
widgets_dir=$app_dir/widgets
font_dir=$data_home/fonts/ghost-shell
unit_dir=$config_home/systemd/user

shell_src=$repo/shell
shaders_src=$shell_src/kit/GhostKit/shaders
widgets_src=$repo/widgets
units_src=$repo/packaging/systemd
kwin_script=$repo/scripts/kwin-script.sh
binary=${CARGO_TARGET_DIR:-$repo/target}/release/ghostd
alerts_binary=${CARGO_TARGET_DIR:-$repo/target}/release/ghost-alerts-inmet

nvidia_dropin_src=$units_src/ghost-shell-nvidia.conf
nvidia_dropin_dir=$unit_dir/ghost-shell.service.d
nvidia_dropin=$nvidia_dropin_dir/nvidia.conf
nvidia_version_file=/proc/driver/nvidia/version
drm_dir=/sys/class/drm
nvidia_drm_driver=nvidia

excluded_widget=broken-example
required_tools=(cargo fc-list fc-cache kpackagetool6 kreadconfig6)

font_url=https://github.com/google/fonts/raw/main/ofl/kodemono
# curl treats [] as a glob and GitHub rejects the literal brackets, so each entry is "<local name> <percent-encoded remote name>".
font_files=(
    'KodeMono[ROND,wdth,wght].ttf KodeMono%5BROND%2Cwdth%2Cwght%5D.ttf'
    'OFL.txt OFL.txt'
)

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

check_prerequisites() {
    local tool frag missing_shaders=()

    for tool in "${required_tools[@]}"; do
        if ! command -v "$tool" > /dev/null 2>&1; then
            die "$tool not found in PATH"
        fi
    done

    if [[ ! -f $shell_src/shell.qml ]]; then
        die "$shell_src/shell.qml is missing; the shell has not been written yet"
    fi
    if [[ ! -d $widgets_src ]]; then
        die "$widgets_src is missing; the widgets have not been written yet"
    fi

    for frag in "$shaders_src"/*.frag; do
        if [[ -e $frag && ! -f $frag.qsb ]]; then
            missing_shaders+=("${frag##*/}")
        fi
    done
    if (( ${#missing_shaders[@]} > 0 )); then
        die "no compiled shader for ${missing_shaders[*]}; run make shaders first"
    fi
}

# --delete-excluded also removes an excluded directory left behind by an older install.
sync_tree() {
    local src=$1 dest=$2 name rsync_args=()
    shift 2

    if command -v rsync > /dev/null 2>&1; then
        for name in "$@"; do
            rsync_args+=("--exclude=/$name")
        done
        mkdir -p "$dest"
        rsync -a --delete --delete-excluded "${rsync_args[@]}" "$src/" "$dest/"
    else
        rm -rf "${dest:?}"
        mkdir -p "$(dirname "$dest")"
        cp -a "$src" "$dest"
        for name in "$@"; do
            rm -rf "${dest:?}/$name"
        done
    fi
}

# Called from an if, where errexit is off, so every failure has to return explicitly.
install_kode_mono() {
    local entry name remote

    if ! mkdir -p "$font_dir"; then
        return 1
    fi
    for entry in "${font_files[@]}"; do
        read -r name remote <<< "$entry"
        if ! curl -fsSL -o "$font_dir/$name.part" "$font_url/$remote"; then
            rm -f "$font_dir/$name.part"
            return 1
        fi
        if ! mv "$font_dir/$name.part" "$font_dir/$name"; then
            return 1
        fi
    done
    fc-cache -f "$font_dir"
}

install_font_if_missing() {
    local installed_fonts
    # Capturing first keeps grep -q from closing the pipe early, which pipefail would report as a miss.
    installed_fonts=$(fc-list)

    if grep -qi 'kode mono' <<< "$installed_fonts"; then
        step "Kode Mono already installed"
        return
    fi

    step "installing Kode Mono into $font_dir"
    if ! install_kode_mono; then
        warn "could not install Kode Mono; the shell will fall back to another font"
    fi
}

# Prints why the NVIDIA drop-in must not be installed, or nothing when it may be. The drop-in points
# the EGL loader at NVIDIA alone, which makes EGL fail outright on a system that renders through Mesa.
nvidia_dropin_blocker() {
    local json_file card driver json_files=() cards=()

    if [[ ! -e $nvidia_version_file ]]; then
        echo "no NVIDIA driver is loaded"
        return 0
    fi

    # Read from the drop-in itself so the files it names and the files checked here cannot drift apart.
    mapfile -t json_files < <(sed -n 's|^Environment=__EGL_[A-Z_]*=\(/.*\.json\)$|\1|p' "$nvidia_dropin_src")
    if (( ${#json_files[@]} == 0 )); then
        echo "$nvidia_dropin_src names no EGL files"
        return 0
    fi
    for json_file in "${json_files[@]}"; do
        if [[ ! -f $json_file ]]; then
            echo "$json_file is missing"
            return 0
        fi
    done

    # cardN-<connector> entries are outputs, not devices.
    for card in "$drm_dir"/card*; do
        if [[ ${card##*/} =~ ^card[0-9]+$ ]]; then
            cards+=("$card")
        fi
    done
    if (( ${#cards[@]} == 0 )); then
        echo "no DRM device was found"
        return 0
    fi
    for card in "${cards[@]}"; do
        driver=$(basename "$(readlink -f "$card/device/driver")")
        if [[ $driver != "$nvidia_drm_driver" ]]; then
            echo "${card##*/} is driven by $driver, not $nvidia_drm_driver"
            return 0
        fi
    done
}

sync_nvidia_dropin() {
    local blocker removed=

    blocker=$(nvidia_dropin_blocker)
    if [[ -z $blocker ]]; then
        step "installing the NVIDIA drop-in into $nvidia_dropin_dir"
        install -D -m 644 "$nvidia_dropin_src" "$nvidia_dropin"
        return
    fi

    if [[ -e $nvidia_dropin ]]; then
        rm -f -- "$nvidia_dropin"
        removed="; removed the copy installed earlier"
    fi
    # Fails, harmlessly, when the directory holds another drop-in or does not exist.
    rmdir "$nvidia_dropin_dir" 2> /dev/null || true
    step "not installing the NVIDIA drop-in: $blocker$removed"
}

install_units() {
    mkdir -p "$unit_dir"
    install -m 644 -t "$unit_dir" "$units_src"/ghostd.service "$units_src"/ghost-shell.service
    sync_nvidia_dropin

    if systemctl --user show-environment > /dev/null 2>&1; then
        systemctl --user daemon-reload
    else
        warn "systemctl --user is unavailable; run 'systemctl --user daemon-reload' from a login session"
    fi
}

# Installs or upgrades the KWin script without ever enabling it, writing kwinrc or calling KWin.
# KWin keeps the code it compiled for a loaded script, even across an unload and start, so an update
# to an enabled script takes effect only when KWin next starts.
install_kwin_script() {
    local was_enabled=

    if "$kwin_script" enabled; then
        was_enabled=1
    fi
    "$kwin_script" install

    if [[ -n $was_enabled ]]; then
        step "the KWin desktop script update takes effect at your next login"
    else
        step "the KWin desktop script is installed but not enabled; enable it with: $kwin_script enable"
    fi
}

check_prerequisites

step "building ghostd and ghost-alerts-inmet (release)"
cargo build --release --quiet --manifest-path "$repo/Cargo.toml"
for built in "$binary" "$alerts_binary"; do
    if [[ ! -x $built ]]; then
        die "$built was not produced; is a custom cargo target directory configured?"
    fi
done

step "installing ghostd and ghost-alerts-inmet into $bin_dir"
install -D -m 755 "$binary" "$bin_dir/ghostd"
# Beside ghostd, which starts it from its own dir because the user manager's PATH lacks this one.
install -D -m 755 "$alerts_binary" "$bin_dir/ghost-alerts-inmet"

step "copying the shell into $shell_dir"
sync_tree "$shell_src" "$shell_dir"

step "copying the widgets into $widgets_dir (without $excluded_widget)"
sync_tree "$widgets_src" "$widgets_dir" "$excluded_widget"

install_font_if_missing

step "installing the user units into $unit_dir"
install_units

step "installing the KWin desktop script"
install_kwin_script

step "done; start with: systemctl --user start ghostd ghost-shell"
