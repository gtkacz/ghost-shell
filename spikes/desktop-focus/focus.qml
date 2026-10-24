import QtQuick
import Quickshell
import Quickshell.Wayland

// The desktop surface as Desktop.qml configures it (Bottom layer, namespace "desktop", all edges, no exclusion, an input
// mask), with one search field and one plain claiming area. SPIKE_MODE:
//   switch      starts at None, becomes OnDemand on a click on the field, None again on Esc
//   hover       as switch, and also OnDemand while the pointer is over the field, so the surface is already OnDemand
//               when the click lands
//   permanent   OnDemand from the start
ShellRoot {
    id: root

    readonly property string mode: Quickshell.env("SPIKE_MODE")
    readonly property bool permanent: mode === "permanent"
    // Whether Esc also unmaps the surface for a moment, to see if KWin then activates the window that was active before.
    readonly property bool remap: Quickshell.env("SPIKE_REMAP") === "1"
    property int focusMode: permanent ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

    function stamp() {
        return " at " + Date.now();
    }

    onFocusModeChanged: console.info("spike desktop keyboardFocus " + focusMode + stamp())

    PanelWindow {
        id: window

        anchors {
            top: true
            bottom: true
            left: true
            right: true
        }
        exclusionMode: ExclusionMode.Ignore
        color: "#101418"
        WlrLayershell.layer: WlrLayer.Bottom
        WlrLayershell.namespace: "desktop"
        WlrLayershell.keyboardFocus: root.focusMode
        mask: Region {
            regions: [fieldRegion, widgetRegion]
        }

        Region {
            id: fieldRegion

            x: field.x
            y: field.y
            width: field.width
            height: field.height
        }

        Region {
            id: widgetRegion

            x: widget.x
            y: widget.y
            width: widget.width
            height: widget.height
        }

        Rectangle {
            id: widget

            x: 1700
            y: 100
            width: 400
            height: 150
            color: "#203040"

            MouseArea {
                anchors.fill: parent
                onClicked: console.info("spike desktop widget click" + root.stamp())
            }
        }

        Rectangle {
            id: field

            x: 1700
            y: 1100
            width: 400
            height: 80
            color: input.activeFocus ? "#304050" : "#202830"

            TextInput {
                id: input

                anchors.fill: parent
                color: "white"
                font.pixelSize: 40
                onTextChanged: console.info("spike desktop text '" + text + "'" + root.stamp())
                onActiveFocusChanged: console.info("spike desktop input activeFocus " + activeFocus + root.stamp())
                Keys.onPressed: event => {
                    console.info("spike desktop key " + event.key + root.stamp());
                    if (event.key === Qt.Key_Escape) {
                        root.focusMode = root.permanent ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None;
                        input.focus = false;
                        if (root.remap) {
                            window.visible = false;
                            remapTimer.start();
                        }
                    }
                }
            }

            // Above the TextInput, which would otherwise take the press itself and never let the click through.
            MouseArea {
                anchors.fill: parent
                hoverEnabled: root.mode === "hover"
                onEntered: root.focusMode = WlrKeyboardFocus.OnDemand
                onExited: {
                    if (!input.activeFocus) {
                        root.focusMode = WlrKeyboardFocus.None;
                    }
                }
                onClicked: {
                    console.info("spike desktop field click" + root.stamp());
                    root.focusMode = WlrKeyboardFocus.OnDemand;
                    input.forceActiveFocus();
                }
            }
        }

        Timer {
            id: remapTimer

            interval: 200
            onTriggered: window.visible = true
        }

        Connections {
            target: field.Window.window

            function onActiveChanged() {
                console.info("spike desktop window active " + field.Window.window.active + root.stamp());
            }
        }
    }

    Component.onCompleted: console.info("spike desktop ready mode=" + mode + stamp())
}
