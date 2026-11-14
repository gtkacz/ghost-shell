import os
import subprocess
from pathlib import Path

import pytest

SCRIPT = Path(__file__).with_name("terminal-shortcut.sh")


@pytest.mark.parametrize("user_override", [False, True])
def test_konsole_default_stays_disabled_without_a_running_session(tmp_path, user_override):
    apps = tmp_path / "system/applications"
    apps.mkdir(parents=True)
    konsole = "[Desktop Entry]\nName=Konsole\nExec=konsole\nX-KDE-Shortcuts=Ctrl+Alt+T\n\n[Desktop Action NewWindow]\nExec=konsole\n"
    (apps / "org.kde.konsole.desktop").write_text(konsole)
    # Plasma loads these before looking for applications with default shortcuts.
    accelerator_entries = apps.parent / "kglobalaccel"
    accelerator_entries.mkdir()
    (accelerator_entries / "org.kde.konsole.desktop").write_text(konsole)
    (apps / "com.mitchellh.ghostty.desktop").write_text(
        "[Desktop Entry]\nName=Ghostty\nExec=ghostty\nX-KDE-Shortcuts=Ctrl+Alt+T\n"
    )
    data = tmp_path / "user"
    if user_override:
        (data / "applications").mkdir(parents=True)
        (data / "applications/org.kde.konsole.desktop").write_text(
            konsole.replace("Name=Konsole", "Name=My Konsole").replace("X-KDE-Shortcuts=Ctrl+Alt+T\n", "")
        )
    env = {**os.environ, "HOME": str(tmp_path), "XDG_DATA_HOME": str(data),
           "XDG_DATA_DIRS": str(apps.parent), "XDG_STATE_HOME": str(tmp_path / "state"),
           "DBUS_SESSION_BUS_ADDRESS": "unix:path=/nonexistent-ghost-test-bus"}
    for _ in range(2):
        result = subprocess.run(["bash", str(SCRIPT)], env=env, capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        override = (data / "applications/org.kde.konsole.desktop").read_text()
        assert "X-KDE-Shortcuts=\n" in override
        assert "Ctrl+Alt+T" not in override
        assert "[Desktop Action NewWindow]\nExec=konsole" in override
        if user_override:
            assert "Name=My Konsole" in override
            assert len(list((tmp_path / "state/ghost-shell/terminal-shortcut").glob("*.desktop"))) == 1
        assert (apps / "org.kde.konsole.desktop").read_text() == konsole
        shortcut_override = (data / "kglobalaccel/org.kde.konsole.desktop").read_text()
        assert "X-KDE-Shortcuts=\n" in shortcut_override
        assert "Ctrl+Alt+T" not in shortcut_override
        assert (accelerator_entries / "org.kde.konsole.desktop").read_text() == konsole
