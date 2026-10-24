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
    // How long a sent brightness is shown before the topic is believed again, if it never echoes it.
    readonly property int holdMs: 1500
    readonly property var noDisplay: ({
            id: "",
            label: "",
            brightness: 0,
            max: 1
        })

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

        // The count, not the array: every message carries a new array, and a model that changes with it would rebuild
        // the rows, and the one being dragged with them.
        Repeater {
            model: root.displays.length

            Column {
                id: display

                required property int index

                readonly property var modelData: index < root.displays.length ? root.displays[index] : root.noDisplay
                // The value under the pointer while it presses, then the one sent last until the topic shows it, so the
                // cells follow the hand and do not snap back to a message that lags it.
                property int dragged: -1
                property int held: -1
                readonly property int published: modelData.brightness
                readonly property real fraction: TrayRules.displayedBrightness(dragged, held, published) / Math.max(1, modelData.max)

                onPublishedChanged: held = TrayRules.pendingAfter(held, published)

                Timer {
                    id: holdLimit

                    interval: root.holdMs
                    onTriggered: display.held = -1
                }

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
                        function release() {
                            display.held = display.dragged;
                            display.dragged = -1;
                            holdLimit.restart();
                        }

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
                        onReleased: release()
                        onCanceled: release()
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
                text: root.nightLight.status
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
