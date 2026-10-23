import QtQml
import QtQuick
import QtQuick.Window

// A second app for Qt's own qml runtime, whose app id differs from Quickshell's fixed one.
// usage: qml plain.qml -- <title prefix> <window count>
Window {
    id: first

    readonly property var spikeArgs: Qt.application.arguments.slice(Qt.application.arguments.indexOf("--") + 1)
    readonly property string titlePrefix: spikeArgs[0] ?? "bar-bridge-b"
    readonly property int count: parseInt(spikeArgs[1] ?? "1")
    readonly property color fill: "#38302a"
    readonly property int widthPx: 360
    readonly property int heightPx: 240

    width: widthPx
    height: heightPx
    color: fill
    title: titlePrefix + "-1"
    visible: true

    Instantiator {
        model: first.count - 1

        Window {
            required property int index

            width: first.widthPx
            height: first.heightPx
            color: first.fill
            title: first.titlePrefix + "-" + (index + 2)
            visible: true
        }
    }
}
