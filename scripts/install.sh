#!/usr/bin/env bash
set -euo pipefail

: "${HOME:?HOME must be set}"

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
data_home=${XDG_DATA_HOME:-$HOME/.local/share}
config_home=${XDG_CONFIG_HOME:-$HOME/.config}

bin_dir=$HOME/.local/bin
app_dir=$data_home/ghost-shell
shell_dir=$app_dir/shell
widgets_dir=$app_dir/widgets
font_dir=$data_home/fonts/ghost-shell
unit_dir=$config_home/systemd/user

shell_src=$repo/shell
shaders_src=$shell_src/kit/GhostKit/shaders
widgets_src=$repo/widgets
units_src=$repo/packaging/systemd
binary=${CARGO_TARGET_DIR:-$repo/target}/release/ghostd

excluded_widget=broken-example
required_tools=(cargo qsb-qt6 fc-list fc-cache)

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

install_units() {
    mkdir -p "$unit_dir"
    install -m 644 -t "$unit_dir" "$units_src"/ghostd.service "$units_src"/ghost-shell.service

    if systemctl --user show-environment > /dev/null 2>&1; then
        systemctl --user daemon-reload
    else
        warn "systemctl --user is unavailable; run 'systemctl --user daemon-reload' from a login session"
    fi
}

check_prerequisites

step "building ghostd (release)"
cargo build --release --quiet --manifest-path "$repo/Cargo.toml"
if [[ ! -x $binary ]]; then
    die "$binary was not produced; is a custom cargo target directory configured?"
fi

step "installing ghostd into $bin_dir"
install -D -m 755 "$binary" "$bin_dir/ghostd"

step "copying the shell into $shell_dir"
sync_tree "$shell_src" "$shell_dir"

step "copying the widgets into $widgets_dir (without $excluded_widget)"
sync_tree "$widgets_src" "$widgets_dir" "$excluded_widget"

install_font_if_missing

step "installing the user units into $unit_dir"
install_units

step "done; start with: systemctl --user start ghostd ghost-shell"
