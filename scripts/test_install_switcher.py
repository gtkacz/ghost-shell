"""Tests install.sh --switcher and --remove-switcher in a throwaway HOME.

Every run builds its environment from nothing and puts a stub qdbus-qt6 first on PATH, so the real one never runs and
no test can reach the live KWin, whatever the session around it looks like.
"""

import configparser
import os
import stat
import subprocess
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
INSTALL = REPO / "scripts/install.sh"
LAYOUT = "org.ghostshell.switcher"
ANSWERS_AT_ONCE = "#!/bin/sh\nexit 0\n"
NO_BUS = "#!/bin/sh\nexit 1\n"
HUNG = "#!/bin/sh\nexec sleep 60\n"


class Box:
    def __init__(self, root: Path):
        self.home = root / "home"
        self.stubs = root / "stubs"
        self.home.mkdir()
        self.stubs.mkdir()
        self.kwinrc = self.home / ".config/kwinrc"
        self.kwinrc.parent.mkdir(parents=True)
        self.package = self.home / ".local/share/kwin/tabbox" / LAYOUT
        self.record = self.home / ".local/share/ghost-shell/switcher.previous"
        self.stub_qdbus(ANSWERS_AT_ONCE)
        self.write("Compositing", "Backend", "OpenGL")

    def stub_qdbus(self, body: str) -> None:
        stub = self.stubs / "qdbus-qt6"
        stub.write_text(body)
        stub.chmod(stub.stat().st_mode | stat.S_IEXEC)

    def bare_env(self) -> dict[str, str]:
        return {"HOME": str(self.home), "PATH": f"{self.stubs}:{os.environ['PATH']}"}

    def env(self) -> dict[str, str]:
        return self.bare_env() | {
            "XDG_CONFIG_HOME": str(self.home / ".config"),
            "XDG_DATA_HOME": str(self.home / ".local/share"),
            "XDG_STATE_HOME": str(self.home / ".local/state"),
            "XDG_RUNTIME_DIR": str(self.home),
        }

    def install(self, *args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
        return subprocess.run(
            [str(INSTALL), *args],
            env=env or self.env(),
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )

    def write(self, group: str, key: str, value: str) -> None:
        subprocess.run(
            ["kwriteconfig6", "--file", "kwinrc", "--group", group, "--key", key, value],
            env=self.env(),
            check=True,
        )

    def group(self, name: str) -> dict[str, str]:
        parser = configparser.ConfigParser(interpolation=None, strict=False)
        parser.optionxform = str
        parser.read(self.kwinrc)
        return dict(parser[name]) if parser.has_section(name) else {}


@pytest.fixture
def box(tmp_path):
    return Box(tmp_path)


def installed(box: Box) -> subprocess.CompletedProcess:
    result = box.install("--switcher")
    assert result.returncode == 0, result.stderr
    return result


def removed(box: Box) -> subprocess.CompletedProcess:
    result = box.install("--remove-switcher")
    assert result.returncode == 0, result.stderr
    return result


def test_install_sets_the_layout_and_turns_highlight_off(box):
    box.write("TabBoxAlternative", "LayoutName", "theirs")
    installed(box)
    assert box.group("TabBox") == {"LayoutName": LAYOUT, "HighlightWindows": "false"}
    assert box.group("TabBoxAlternative") == {"LayoutName": "theirs"}
    assert (box.package / "contents/ui/main.qml").is_file()
    assert (box.package / "contents/ui/GhostGate/qmldir").is_file()
    assert (box.package / "contents/ui/GhostGate/shaders/duotone.frag.qsb").is_file()
    assert (box.package / "metadata.json").is_file()


def test_remove_restores_kwinrc_byte_for_byte_when_the_keys_were_unset(box):
    before = box.kwinrc.read_bytes()
    installed(box)
    assert box.kwinrc.read_bytes() != before
    removed(box)
    assert box.kwinrc.read_bytes() == before
    assert not box.package.exists()
    assert not box.record.exists()


def test_remove_restores_kwinrc_byte_for_byte_when_the_keys_were_set(box):
    box.write("TabBox", "LayoutName", "thumbnail_grid")
    box.write("TabBox", "HighlightWindows", "true")
    before = box.kwinrc.read_bytes()
    installed(box)
    assert box.group("TabBox")["LayoutName"] == LAYOUT
    removed(box)
    assert box.kwinrc.read_bytes() == before


def in_a_plasma_session(box: Box) -> dict[str, str]:
    """kdedefaults holds the layout, and is in view as a Plasma session has it: without this KConfig never reads it."""
    defaults = box.home / ".config/kdedefaults/kwinrc"
    defaults.parent.mkdir()
    defaults.write_text("[TabBox]\nLayoutName=org.kde.breeze.desktop\n")
    return box.env() | {"XDG_CONFIG_DIRS": str(defaults.parent)}


def test_kdedefaults_does_not_count_as_set(box):
    session = in_a_plasma_session(box)
    before = box.kwinrc.read_bytes()
    assert box.install("--switcher", env=session).returncode == 0
    assert "LayoutName unset" in box.record.read_text()
    assert box.install("--remove-switcher", env=session).returncode == 0
    assert box.kwinrc.read_bytes() == before


def test_remove_restores_a_value_equal_to_its_kdedefault(box):
    session = in_a_plasma_session(box)
    box.write("TabBox", "LayoutName", "org.kde.breeze.desktop")
    before = box.kwinrc.read_bytes()
    assert box.install("--switcher", env=session).returncode == 0
    assert "LayoutName=org.kde.breeze.desktop" in box.record.read_text()
    assert box.install("--remove-switcher", env=session).returncode == 0
    assert box.kwinrc.read_bytes() == before


def test_a_second_install_keeps_the_first_record(box):
    box.write("TabBox", "LayoutName", "thumbnail_grid")
    installed(box)
    first = box.record.read_text()
    assert "LayoutName=thumbnail_grid" in first
    installed(box)
    assert box.record.read_text() == first
    removed(box)
    assert box.group("TabBox")["LayoutName"] == "thumbnail_grid"


def test_remove_leaves_a_layout_the_user_chose(box):
    box.write("TabBox", "HighlightWindows", "true")
    installed(box)
    box.write("TabBox", "LayoutName", "thumbnail_grid")
    result = removed(box)
    assert box.group("TabBox") == {"LayoutName": "thumbnail_grid", "HighlightWindows": "true"}
    assert "thumbnail_grid" in result.stderr


def test_a_failed_assemble_keeps_the_working_package(box):
    installed(box)
    before = box.kwinrc.read_bytes()
    result = box.install("--switcher", env=box.env() | {"GHOST_SWITCHER_ASSEMBLE": "/bin/false"})
    assert result.returncode != 0
    assert (box.package / "contents/ui/main.qml").is_file()
    assert (box.package / "contents/ui/GhostGate/qmldir").is_file()
    assert box.kwinrc.read_bytes() == before


def test_a_failed_first_assemble_changes_nothing(box):
    before = box.kwinrc.read_bytes()
    result = box.install("--switcher", env=box.env() | {"GHOST_SWITCHER_ASSEMBLE": "/bin/false"})
    assert result.returncode != 0
    assert box.kwinrc.read_bytes() == before
    assert not box.record.exists()
    assert not box.package.exists()
    assert list(box.package.parent.glob("*")) == []


def test_reconfigure_failing_only_warns(box):
    box.stub_qdbus(NO_BUS)
    result = installed(box)
    assert "warning:" in result.stderr
    assert box.group("TabBox")["LayoutName"] == LAYOUT
    result = removed(box)
    assert "next start" in result.stderr


def test_remove_works_with_no_display_and_no_bus(box):
    box.stub_qdbus(NO_BUS)
    before = box.kwinrc.read_bytes()
    installed(box)
    result = box.install("--remove-switcher", env=box.bare_env())
    assert result.returncode == 0, result.stderr
    assert box.kwinrc.read_bytes() == before
    assert not box.package.exists()


def test_a_hung_kwin_cannot_hang_removal(box):
    before = box.kwinrc.read_bytes()
    installed(box)
    box.stub_qdbus(HUNG)
    result = subprocess.run(
        [str(INSTALL), "--remove-switcher"],
        env=box.env(),
        capture_output=True,
        text=True,
        timeout=15,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert box.kwinrc.read_bytes() == before


def test_remove_with_no_record_leaves_kwinrc_alone(box):
    box.write("TabBox", "LayoutName", "thumbnail_grid")
    before = box.kwinrc.read_bytes()
    result = removed(box)
    assert box.kwinrc.read_bytes() == before
    assert "no record" in result.stdout
