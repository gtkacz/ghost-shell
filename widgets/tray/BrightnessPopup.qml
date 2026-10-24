import QtQuick
import GhostKit

// The sun icon's popup in the panel style: one cell slider per display, then the Night Light switch.
Panel {
    id: root

    // Design pixels, from the reference mockup's .tip type and the panel's own rhythm.
    readonly property real widthPx: 232
    readonly property real textPx: 9.5
    readonly property real rowGapPx: 10
    readonly property real captionGapPx: 4
    readonly property real sliderHitPx: 14
    readonly property real switchWidthPx: 28
    readonly property real switchHeightPx: 12
    readonly property real switchInsetPx: 2
    readonly property real switchGapPx: 8
    // Letter spacing in em.
    readonly property real tracking: 0.04
    readonly property string noDisplaysText: "NO DISPLAYS"

    // The brightness topic's value, or null before its first message.
    property var snapshot: null
    readonly property var displays: snapshot !== null ? snapshot.displays : []
    readonly property var nightLight: TrayRules.nightLightSwitch(snapshot !== null ? snapshot.night_light : null)

    signal displaySet(string id, int value)
    signal nightLightToggled

    readonly property font textFont: Qt.font({
        family: Theme.font,
        pixelSize: Theme.d(textPx),
        weight: Font.DemiBold,
        letterSpacing: Theme.d(textPx) * tracking
    })

    // The popup floats over widgets, so it takes the tips' near-opaque ground rather than the panel's translucent fill.
    fill: Qt.alpha(Theme.ground, 0.96)
    label: "Brightness"
    tagKey: "bright"
    implicitWidth: Theme.d(widthPx)
    implicitHeight: Theme.d(headerHeightPx + bodyTopPx) + rows.implicitHeight + padding

    Column {
        id: rows

        width: parent.width
        spacing: Theme.d(root.rowGapPx)

        Text {
            visible: root.displays.length === 0
            text: root.noDisplaysText
            color: Theme.dim
            font: root.textFont
        }

        Repeater {
            model: root.displays

            Column {
                id: display

                required property var modelData

                // The value under the pointer, so the cells follow it before the daemon's message comes back.
                property int dragged: -1
                readonly property real fraction: (dragged >= 0 ? dragged : modelData.brightness) / Math.max(1, modelData.max)

                width: rows.width
                spacing: Theme.d(root.captionGapPx)

                Item {
                    width: parent.width
                    height: caption.implicitHeight

                    Text {
                        id: caption

                        width: parent.width - percent.implicitWidth - Theme.d(root.switchGapPx)
                        text: display.modelData.label.toUpperCase()
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: Theme.text
                        font: root.textFont
                    }

                    Text {
                        id: percent

                        anchors.right: parent.right
                        text: Math.round(display.fraction * 100) + "%"
                        color: Theme.hi
                        font: root.textFont
                    }
                }

                Item {
                    width: parent.width
                    height: Theme.d(root.sliderHitPx)

                    SegMeter {
                        id: meter

                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width
                        value: display.fraction
                        // Brightness is no load, so no level of it is an alarm.
                        warnAt: 2
                        critAt: 2
                    }

                    MouseArea {
                        function setAt(x) {
                            const value = TrayRules.cellBrightness(x, meter.cells, meter.cellStep, display.modelData.max);
                            if (value !== display.dragged) {
                                display.dragged = value;
                                root.displaySet(display.modelData.id, value);
                            }
                        }

                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onPressed: mouse => setAt(mouse.x)
                        onPositionChanged: mouse => setAt(mouse.x)
                        onReleased: display.dragged = -1
                        onCanceled: display.dragged = -1
                    }
                }
            }
        }

        Item {
            width: parent.width
            height: Math.max(nightLabel.implicitHeight, Theme.d(root.switchHeightPx))

            Text {
                id: nightLabel

                anchors.verticalCenter: parent.verticalCenter
                text: "NIGHT LIGHT"
                color: Theme.text
                font: root.textFont
            }

            Text {
                anchors {
                    right: toggle.left
                    rightMargin: Theme.d(root.switchGapPx)
                    verticalCenter: parent.verticalCenter
                }
                text: root.nightLight.on ? "ON" : (root.snapshot !== null && !root.snapshot.night_light.available ? "UNAVAILABLE" : (root.nightLight.usable ? "OFF" : "DISABLED"))
                color: root.nightLight.on ? Theme.signal : Theme.dim
                font: root.textFont
            }

            Rectangle {
                id: toggle

                anchors {
                    right: parent.right
                    verticalCenter: parent.verticalCenter
                }
                width: Theme.d(root.switchWidthPx)
                height: Theme.d(root.switchHeightPx)
                color: "transparent"
                border {
                    width: Theme.d(1)
                    color: root.nightLight.usable ? Theme.signal : Theme.hair
                }

                Rectangle {
                    x: root.nightLight.on ? parent.width - width - Theme.d(root.switchInsetPx) : Theme.d(root.switchInsetPx)
                    y: Theme.d(root.switchInsetPx)
                    width: (parent.width - 3 * Theme.d(root.switchInsetPx)) / 2
                    height: parent.height - 2 * Theme.d(root.switchInsetPx)
                    color: root.nightLight.on ? Theme.signal : Theme.dim
                }
            }

            MouseArea {
                anchors.fill: parent
                enabled: root.nightLight.usable
                cursorShape: Qt.PointingHandCursor
                onClicked: root.nightLightToggled()
            }
        }
    }
}
