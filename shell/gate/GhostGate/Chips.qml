import QtQuick
import "lib/gate.mjs" as GateLib
import "lib/theme.mjs" as ThemeLib

// The layout chip, the Caps Lock chip and, at login, the session chip with its list.
Item {
    id: root

    // Design pixels, from the reference mockup's .chip.
    readonly property real textPx: 9
    readonly property real linePx: 15
    readonly property real sidePx: 6
    readonly property real innerGapPx: 5
    readonly property real gapPx: 6
    readonly property real iconPx: 12
    readonly property real chevronPx: 10
    readonly property real listPaddingPx: 6
    // Letter spacing in em.
    readonly property real tracking: 0.08
    // The mockup's border strengths, as percentages like withAlpha() takes.
    readonly property real borderAlphaPct: 35
    readonly property real warnBorderAlphaPct: 50

    property var adapter
    property bool sessionsOpen: false
    readonly property bool login: adapter.mode === "login"
    readonly property bool canPickSession: adapter.sessions.length > 1

    implicitHeight: Theme.d(linePx + 2)

    component ChipBox: Rectangle {
        id: box

        property string label
        property string glyph
        property color tone: Theme.signal
        property real borderPct: root.borderAlphaPct
        property bool clickable: true
        property string trailingGlyph
        signal clicked

        implicitWidth: line.implicitWidth + Theme.d(2 * (root.sidePx + 1))
        implicitHeight: Theme.d(root.linePx + 2)
        color: "transparent"
        border {
            width: Theme.d(1)
            color: box.clickable && area.containsMouse ? Theme.signal : ThemeLib.withAlpha(box.tone, box.borderPct)
        }

        Row {
            id: line

            anchors.centerIn: parent
            spacing: Theme.d(root.innerGapPx)

            Glyph {
                visible: box.glyph !== ""
                anchors.verticalCenter: parent.verticalCenter
                // Glyph warns about an unknown name, so an absent icon still names a real one.
                name: visible ? box.glyph : "keyboard"
                size: Theme.d(root.iconPx)
                color: box.tone
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: box.label
                textFormat: Text.PlainText
                color: box.tone
                font {
                    family: Theme.font
                    pixelSize: Theme.d(root.textPx)
                    weight: Font.Medium
                    letterSpacing: Theme.d(root.textPx) * root.tracking
                    capitalization: Font.AllUppercase
                }
            }

            Glyph {
                visible: box.trailingGlyph !== ""
                anchors.verticalCenter: parent.verticalCenter
                name: visible ? box.trailingGlyph : "keyboard"
                size: Theme.d(root.chevronPx)
                color: box.tone
            }
        }

        MouseArea {
            id: area

            anchors.fill: parent
            hoverEnabled: true
            enabled: box.clickable
            cursorShape: Qt.PointingHandCursor
            onClicked: box.clicked()
        }
    }

    Row {
        spacing: Theme.d(root.gapPx)

        ChipBox {
            objectName: "layoutChip"
            visible: root.adapter.layout !== ""
            label: root.adapter.layout
            glyph: "keyboard"
            clickable: root.adapter.layouts.length > 1
            onClicked: root.adapter.cycleLayout()
        }

        ChipBox {
            objectName: "capsChip"
            visible: root.adapter.capsLock
            label: "CAPS LOCK"
            tone: Theme.warn
            borderPct: root.warnBorderAlphaPct
            clickable: false
        }
    }

    ChipBox {
        id: sessionChip

        objectName: "sessionChip"
        anchors.right: parent.right
        visible: root.login
        label: root.adapter.sessions.length > 0 ? root.adapter.sessions[root.adapter.sessionIndex].name : ""
        trailingGlyph: "chevron-down"
        clickable: root.canPickSession
        onClicked: root.sessionsOpen = !root.sessionsOpen
    }

    Rectangle {
        visible: root.login && root.sessionsOpen
        z: 1
        anchors {
            right: sessionChip.right
            bottom: sessionChip.top
            bottomMargin: Theme.d(root.gapPx)
        }
        width: list.implicitWidth + Theme.d(2 * root.listPaddingPx)
        height: list.implicitHeight + Theme.d(2 * root.listPaddingPx)
        color: Theme.panel
        border {
            width: Theme.d(1)
            color: Theme.hair
        }

        Column {
            id: list

            anchors.centerIn: parent
            spacing: Theme.d(root.gapPx)

            Repeater {
                model: root.adapter.sessions

                Text {
                    required property var modelData
                    required property int index

                    text: modelData.name
                    textFormat: Text.PlainText
                    color: index === root.adapter.sessionIndex ? Theme.signal : Theme.dim
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.textPx)
                        letterSpacing: Theme.d(root.textPx) * root.tracking
                        capitalization: Font.AllUppercase
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: {
                            root.adapter.chooseSession(parent.index);
                            root.sessionsOpen = false;
                        }
                    }
                }
            }
        }
    }
}
