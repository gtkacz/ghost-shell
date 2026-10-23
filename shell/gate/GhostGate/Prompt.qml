import QtQuick
import "lib/gate.mjs" as GateLib
import "lib/theme.mjs" as ThemeLib

// Style A: one chamfered panel centred on the dot grid, and the action row near the bottom.
Item {
    id: root

    // Design pixels, from the reference mockup's .A.
    readonly property real panelWidthPx: 380
    readonly property real bodySpacingPx: 10
    readonly property real panelPaddingPx: 14
    readonly property real nameTextPx: 18
    readonly property real tagTextPx: 10
    readonly property real whoGapPx: 10
    readonly property real chipSidePx: 6
    readonly property real actionsBottomPx: 34
    // Letter spacing in em.
    readonly property real nameTracking: 0.04
    readonly property real tagTracking: 0.14
    // The mockup's chip border strength, as a percentage like withAlpha() takes.
    readonly property real borderAlphaPct: 35

    property var adapter
    property bool alerted: false
    property date now: new Date()
    readonly property bool lock: adapter.mode === "lock"
    readonly property bool manyUsers: adapter.users.length > 1

    function refocus() {
        password.focusField();
    }

    function raiseAlert() {
        alerted = true;
        alert.restart();
    }

    anchors.fill: parent

    Component.onCompleted: refocus()

    Connections {
        target: root.adapter

        function onFailed() {
            password.clear();
            root.raiseAlert();
        }

        function onSucceeded() {
            password.clear();
        }

        // A backend that stops without a verdict must not leave the password behind in the hidden field.
        function onBusyChanged() {
            if (!root.adapter.busy) {
                password.clear();
            }
        }

        function onMessageChanged() {
            if (root.adapter.message !== "") {
                root.raiseAlert();
            }
        }
    }

    Timer {
        interval: GateLib.minuteDelay(root.now)
        repeat: true
        running: true
        onTriggered: root.now = new Date()
    }

    Panel {
        id: panel

        // Centred as if the alert were absent, so it grows the panel downward and nothing above it moves (spec 6.2).
        readonly property real idleHeight: height - (alert.visible ? alert.implicitHeight + body.spacing : 0)

        anchors.horizontalCenter: parent.horizontalCenter
        y: (parent.height - idleHeight) / 2
        width: Theme.d(root.panelWidthPx)
        height: Theme.d(headerHeightPx + bodyTopPx) + padding + body.implicitHeight
        padding: Theme.d(root.panelPaddingPx)
        label: "Authenticate"
        tagKey: "auth"
        meta: GateLib.metaText(root.adapter.mode, root.now)

        Column {
            id: body

            width: parent.width
            spacing: Theme.d(root.bodySpacingPx)

            Item {
                width: parent.width
                height: userName.height

                Item {
                    id: userName

                    readonly property bool chipped: root.manyUsers

                    objectName: "userName"
                    width: nameText.implicitWidth + (chipped ? Theme.d(2 * root.chipSidePx) : 0)
                    height: nameText.implicitHeight

                    Rectangle {
                        visible: userName.chipped
                        anchors.fill: parent
                        color: "transparent"
                        border {
                            width: Theme.d(1)
                            color: ThemeLib.withAlpha(Theme.signal, root.borderAlphaPct)
                        }
                    }

                    Text {
                        id: nameText

                        anchors.centerIn: parent
                        text: root.adapter.userName
                        textFormat: Text.PlainText
                        color: Theme.signal
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.nameTextPx)
                            weight: Font.Bold
                            letterSpacing: Theme.d(root.nameTextPx) * root.nameTracking
                        }
                    }

                    MouseArea {
                        anchors.fill: parent
                        enabled: userName.chipped
                        onClicked: root.adapter.cycleUser()
                    }
                }

                Text {
                    anchors {
                        left: userName.right
                        leftMargin: Theme.d(root.whoGapPx)
                    }
                    y: nameText.y + userName.y + nameText.baselineOffset - baselineOffset
                    text: root.lock ? Theme.host : GateLib.sessionTag(root.adapter.sessions.length > 0 ? root.adapter.sessions[root.adapter.sessionIndex].name : "")
                    textFormat: Text.PlainText
                    color: Theme.dim
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.tagTextPx)
                        weight: Font.Medium
                        letterSpacing: Theme.d(root.tagTextPx) * root.tagTracking
                        capitalization: Font.AllUppercase
                    }
                }
            }

            Password {
                id: password

                objectName: "password"
                busy: root.adapter.busy
                onSubmitted: (value) => root.adapter.submit(value)
                onLengthChanged: {
                    if (length > 0) {
                        root.alerted = false;
                    }
                }
            }

            GateAlert {
                id: alert

                objectName: "alert"
                width: parent.width
                shown: root.alerted
                text: GateLib.failureText(root.adapter.attempts, root.adapter.message)
            }

            Chips {
                width: parent.width
                adapter: root.adapter
            }
        }
    }

    Actions {
        objectName: "actions"
        adapter: root.adapter
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.d(root.actionsBottomPx)
        }
    }
}
