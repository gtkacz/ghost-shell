import QtQuick
import org.kde.plasma.private.keyboardindicator as KeyboardIndicator

// Loaded apart so a missing Plasma module cannot take the theme down with it.
KeyboardIndicator.KeyState {
    key: Qt.Key_CapsLock
    onLockedChanged: console.log("SPIKE KeyState capsLock.locked changed = " + locked)
}
