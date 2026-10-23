import QtQuick
import Quickshell

// App A: SPIKE_COUNT toplevels from one qs process, so they share Quickshell's app id.
ShellRoot {
    id: root

    readonly property string titlePrefix: Quickshell.env("SPIKE_TITLE") ?? "bar-bridge-a"
    readonly property int count: parseInt(Quickshell.env("SPIKE_COUNT") ?? "1")
    readonly property color fill: "#2a3438"
    readonly property int widthPx: 360
    readonly property int heightPx: 240

    Variants {
        model: Array.from({ length: root.count }, (_, index) => index + 1)

        FloatingWindow {
            required property int modelData

            implicitWidth: root.widthPx
            implicitHeight: root.heightPx
            color: root.fill
            title: root.titlePrefix + "-" + modelData
        }
    }
}
