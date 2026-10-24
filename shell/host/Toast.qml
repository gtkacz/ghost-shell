import QtQuick
import QtQuick.Shapes
import Quickshell
import GhostKit
import "../kit/GhostKit/lib/notifications.mjs" as Rules
import "../kit/GhostKit/lib/theme.mjs" as ThemeLib

// One notification card under the bar. It ends itself only by saying so (timedOut, dismissRequested, activated); the
// host decides what each means. A record that is no longer in the history is gone from the host's list within the
// same turn, so the blank record only keeps the bindings quiet until the card is destroyed.
Item {
    id: root

    // Design pixels, from the notifications mockup's .toast, .row, .acts and .life.
    readonly property real fillPct: 96
    readonly property real headerPx: 22
    readonly property real headerTopPx: 7
    readonly property real headerBottomPx: 4
    readonly property real iconPx: 20
    readonly property real imagePx: 40
    readonly property real imageCutPx: 5
    readonly property real rowGapPx: 10
    readonly property real textPx: 10
    readonly property real textLineHeight: 1.45
    readonly property real textTopPx: 2
    readonly property int bodyLines: 2
    readonly property real crossPx: 10
    readonly property real crossTopPx: 1
    readonly property real chipsTopPx: 8
    readonly property real chipGapPx: 6
    readonly property real chipTextPx: 8.5
    readonly property real chipPadYPx: 2
    readonly property real chipPadXPx: 8
    readonly property int lifeCells: 24
    readonly property real lifeCellHeightPx: 2
    readonly property real lifeGapPx: 2
    readonly property real lifeSidePx: 12
    readonly property real lifeBottomPx: 4
    readonly property real lifeUnlitPct: 8
    // Letter spacing in em.
    readonly property real textTracking: 0.08
    readonly property real chipTracking: 0.14

    readonly property var blank: ({
            id: 0,
            app: "",
            desktop_entry: "",
            app_icon: "",
            image: "",
            summary: "",
            body: "",
            body_text: "",
            urgency: "normal",
            level: "none",
            actions: [],
            arrived_s: 0,
            revision: 0
        })

    property var record
    property int timeoutS: 6
    property bool showIcon: true

    readonly property var rec: record ?? blank
    readonly property bool critical: rec.urgency === "critical"
    readonly property bool boxed: rec.level !== "none"
    readonly property bool timed: !critical
    readonly property bool hovered: hover.hovered
    readonly property bool hasImage: showIcon && rec.image !== ""
    readonly property string iconName: rec.app_icon !== "" ? rec.app_icon : rec.desktop_entry
    readonly property bool hasIcon: hasImage || (showIcon && iconName !== "")
    readonly property real iconBoxPx: hasImage ? imagePx : iconPx
    // The alert shows the body after the summary unless the body is the countdown, which it shows apart.
    readonly property bool countdown: Rules.isCountdown(rec.body_text)
    readonly property string alertText: rec.body_text === "" || countdown ? rec.summary : rec.summary + " — " + rec.body_text
    // The seconds the toast has been held on screen for, in whole steps; hovering stops it.
    property int elapsedS: 0
    readonly property int litCells: Math.max(0, Math.ceil(lifeCells * (1 - elapsedS / timeoutS)))
    // Reading the clock makes the text follow it while a normal toast holds it.
    readonly property string age: {
        FrameClock.epochSecond;
        return Rules.rowText(rec, Math.floor(Date.now() / 1000)).age;
    }
    // Counts the replacements of the record, which each give the toast a fresh life line, even one that changes nothing.
    readonly property int revision: rec.revision ?? 0

    // Only changes when the buttons do: every publish hands over a new array, and a Repeater on it would rebuild the
    // buttons, losing a press in flight.
    property var actions: []
    property bool holdsSeconds: false
    property bool timedOutOnce: false
    property real lastSecond: 0

    signal timedOut
    signal dismissRequested
    signal activated(string key)

    function holdSeconds(wanted) {
        if (wanted === holdsSeconds) {
            return;
        }
        if (wanted) {
            FrameClock.acquireSeconds();
            lastSecond = FrameClock.epochSecond;
        } else {
            FrameClock.releaseSeconds();
        }
        holdsSeconds = wanted;
    }

    onRecChanged: actions = Rules.stableActions(actions, rec.actions)
    onTimedChanged: holdSeconds(timed)
    onRevisionChanged: {
        elapsedS = 0;
        timedOutOnce = false;
    }
    Component.onCompleted: {
        actions = rec.actions;
        holdSeconds(timed);
    }
    Component.onDestruction: holdSeconds(false)

    implicitHeight: Theme.d(headerPx + card.bodyTopPx + card.paddingPx) + content.implicitHeight

    // Under the card, so the buttons and the links above it take their own clicks; a click anywhere else on it opens.
    MouseArea {
        z: -1
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.MiddleButton
        cursorShape: Qt.PointingHandCursor
        onClicked: mouse => {
            if (mouse.button === Qt.MiddleButton) {
                root.dismissRequested();
            } else {
                root.activated("default");
            }
        }
    }

    HoverHandler {
        id: hover
    }

    // Counts the seconds the clock reports, which can be more than one if the shell stalled.
    Connections {
        target: FrameClock
        enabled: root.holdsSeconds

        function onEpochSecondChanged() {
            const passed = FrameClock.epochSecond - root.lastSecond;
            root.lastSecond = FrameClock.epochSecond;
            if (root.hovered) {
                return;
            }
            root.elapsedS += passed;
            if (root.elapsedS >= root.timeoutS && !root.timedOutOnce) {
                root.timedOutOnce = true;
                root.timedOut();
            }
        }
    }

    Panel {
        id: card

        anchors.fill: parent
        fill: ThemeLib.withAlpha(Theme.ground, root.fillPct)
        headerHeightPx: root.headerPx
        headerTopPx: root.headerTopPx
        headerBottomPx: root.headerBottomPx
        label: root.rec.app.toUpperCase()
        meta: root.age

        Column {
            id: content

            width: parent.width

            Item {
                id: row

                visible: !root.boxed
                width: parent.width
                height: root.boxed ? 0 : Math.max(textColumn.implicitHeight, iconBox.height)

                Item {
                    id: iconBox

                    width: root.hasIcon ? Theme.d(root.iconBoxPx) : 0
                    height: root.hasIcon ? width : 0
                    visible: root.hasIcon

                    AppIcon {
                        anchors.fill: parent
                        visible: !root.hasImage
                        name: root.iconName
                        size: Theme.d(root.iconPx)
                    }

                    Image {
                        anchors.fill: parent
                        visible: root.hasImage
                        source: root.hasImage ? "file://" + root.rec.image : ""
                        sourceSize: Qt.size(width * (QsWindow.window?.devicePixelRatio ?? 1), height * (QsWindow.window?.devicePixelRatio ?? 1))
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }

                    // The mockup cuts the picture's top-right and bottom-left corners; the card's own ground is drawn over them.
                    Shape {
                        anchors.fill: parent
                        visible: root.hasImage
                        preferredRendererType: Shape.CurveRenderer

                        ShapePath {
                            fillColor: Theme.ground
                            strokeColor: "transparent"
                            startX: parent.width - Theme.d(root.imageCutPx)
                            startY: 0

                            PathLine {
                                x: iconBox.width
                                y: 0
                            }
                            PathLine {
                                x: iconBox.width
                                y: Theme.d(root.imageCutPx)
                            }
                        }

                        ShapePath {
                            fillColor: Theme.ground
                            strokeColor: "transparent"
                            startX: 0
                            startY: iconBox.height - Theme.d(root.imageCutPx)

                            PathLine {
                                x: 0
                                y: iconBox.height
                            }
                            PathLine {
                                x: Theme.d(root.imageCutPx)
                                y: iconBox.height
                            }
                        }
                    }
                }

                Column {
                    id: textColumn

                    x: iconBox.width + (root.hasIcon ? Theme.d(root.rowGapPx) : 0)
                    width: row.width - x - cross.width - Theme.d(root.rowGapPx)

                    ScrambleText {
                        width: parent.width
                        topPadding: Theme.d(root.textTopPx)
                        text: root.rec.summary
                        // The id, not the text: a replacement rewrites the text and must not scramble again.
                        scrambleKey: "toast:" + root.rec.id
                        color: Theme.hi
                        elide: Text.ElideRight
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.textPx)
                            weight: Font.DemiBold
                            letterSpacing: Theme.d(root.textPx) * root.textTracking
                        }
                    }

                    Text {
                        width: parent.width
                        topPadding: Theme.d(root.textTopPx)
                        visible: root.rec.body_text !== ""
                        // The daemon keeps only b, i, u and a, so the markup cannot do more than style the text.
                        text: root.rec.body
                        textFormat: Text.StyledText
                        wrapMode: Text.Wrap
                        maximumLineCount: root.bodyLines
                        elide: Text.ElideRight
                        lineHeight: root.textLineHeight
                        lineHeightMode: Text.ProportionalHeight
                        color: Theme.text
                        linkColor: Theme.signal
                        font {
                            family: Theme.font
                            pixelSize: Theme.d(root.textPx)
                            weight: Font.Medium
                            letterSpacing: Theme.d(root.textPx) * root.textTracking
                        }
                        onLinkActivated: link => {
                            if (Rules.isOpenableLink(link)) {
                                Launch.url(link);
                            }
                        }
                    }
                }

                Item {
                    id: cross

                    anchors.right: parent.right
                    y: Theme.d(root.crossTopPx)
                    width: Theme.d(root.crossPx)
                    height: width

                    Glyph {
                        anchors.fill: parent
                        name: "cross"
                        size: Theme.d(root.crossPx)
                        color: Theme.dim
                    }

                    MouseArea {
                        anchors.fill: parent
                        anchors.margins: -Theme.d(root.chipPadXPx / 2)
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.dismissRequested()
                    }
                }
            }

            AlertBox {
                visible: root.boxed
                width: parent.width
                level: root.rec.level
                chipKey: "alert"
                // The level is part of the name, so a replacement that raises a warning to critical draws attention again.
                alertKey: "notif:" + root.rec.id + ":" + root.rec.level
                text: root.alertText
                trailing: root.countdown ? root.rec.body_text : ""
            }

            Item {
                visible: !root.boxed && root.actions.length > 0
                width: parent.width
                height: visible ? Theme.d(root.chipsTopPx) + chips.height : 0

                Row {
                    id: chips

                    y: Theme.d(root.chipsTopPx)
                    spacing: Theme.d(root.chipGapPx)

                    Repeater {
                        model: root.actions

                        Rectangle {
                            id: chip

                            required property var modelData
                            required property int index

                            width: chipLabel.implicitWidth + Theme.d(2 * root.chipPadXPx + 2)
                            height: chipLabel.implicitHeight + Theme.d(2 * root.chipPadYPx + 2)
                            color: "transparent"
                            border {
                                width: Theme.d(1)
                                color: index === 0 ? Theme.signal : Theme.hair
                            }

                            Text {
                                id: chipLabel

                                anchors.centerIn: parent
                                text: chip.modelData.label
                                textFormat: Text.PlainText
                                color: chip.index === 0 ? Theme.hi : Theme.text
                                font {
                                    family: Theme.font
                                    pixelSize: Theme.d(root.chipTextPx)
                                    weight: Font.Medium
                                    letterSpacing: Theme.d(root.chipTextPx) * root.chipTracking
                                    capitalization: Font.AllUppercase
                                }
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.activated(chip.modelData.key)
                            }
                        }
                    }
                }
            }
        }
    }

    Row {
        visible: root.timed
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            leftMargin: Theme.d(root.lifeSidePx)
            rightMargin: Theme.d(root.lifeSidePx)
            bottomMargin: Theme.d(root.lifeBottomPx)
        }
        height: Theme.d(root.lifeCellHeightPx)
        spacing: Theme.d(root.lifeGapPx)

        Repeater {
            model: root.lifeCells

            Rectangle {
                required property int index

                width: (parent.width - (root.lifeCells - 1) * parent.spacing) / root.lifeCells
                height: Theme.d(root.lifeCellHeightPx)
                color: index < root.litCells ? Theme.signal : ThemeLib.withAlpha(Theme.signal, root.lifeUnlitPct)
            }
        }
    }
}
