import QtQuick
import GhostKit
import "uplink.mjs" as Uplink

// The machine's live connections on the kit's dot globe, after the reference mockup's drawGlobe(): an arc from home to
// each endpoint the daemon locates, the endpoint's name beside its square, the host's name beside home, and the count
// with the GeoIP licence credit below.
Panel {
    id: root

    // Design pixels, from the reference mockup's drawGlobe(). The offsets put a label's baseline beside its square.
    readonly property real labelPx: 9
    readonly property real smallLabelPx: 8
    readonly property real labelDxPx: 6
    readonly property real labelBaselinePx: 3
    readonly property real homeDxPx: 9
    readonly property real homeBaselinePx: -6
    readonly property real footerPx: 9
    readonly property real footerInsetPx: 10
    readonly property real labelOpacity: 0.75
    // How far a label must overlap a busier one before it hides, or clear it before it returns.
    readonly property real labelMarginPx: 1

    property var settings

    readonly property string topicName: "uplink"
    readonly property var uplink: Data.topic(topicName).value
    readonly property var endpoints: Uplink.shownEndpoints(uplink)
    readonly property var location: Data.config ? Data.config.location : null
    readonly property bool live: Data.connected && !Data.stale && Data.topic(topicName).live
    readonly property bool wantsGlobe: visible && !Data.covered
    // A stale globe stands still, while the want stays so that the topic is replayed once it returns.
    readonly property bool wantsClock: wantsGlobe && live
    readonly property color labelColor: live ? Qt.rgba(Theme.hi.r, Theme.hi.g, Theme.hi.b, labelOpacity) : Theme.dim
    readonly property string homeText: Uplink.homeLabel(Data.topic("host").value)
    // Home first, then one per endpoint; set from the label rects each time the globe moves.
    property var labelsShown: []

    property bool holdsTopic: false
    property bool holdsClock: false

    // Once per change rather than per frame: every change of the subscription makes the daemon replay each topic.
    function holdTopic() {
        if (wantsGlobe === holdsTopic) {
            return;
        }
        if (wantsGlobe) {
            Data.want(topicName);
        } else {
            Data.release(topicName);
        }
        holdsTopic = wantsGlobe;
    }

    function holdClock() {
        if (wantsClock === holdsClock) {
            return;
        }
        if (wantsClock) {
            FrameClock.acquire();
        } else {
            FrameClock.release();
        }
        holdsClock = wantsClock;
    }

    anchors.fill: parent
    label: "Uplink"
    tagKey: "scope"

    // A hold changes a turn later, never inside the update of the binding that asked for it.
    onWantsGlobeChanged: Qt.callLater(holdTopic)
    onWantsClockChanged: Qt.callLater(holdClock)
    onHomeTextChanged: area.placeLabels()
    Component.onCompleted: {
        holdTopic();
        holdClock();
    }
    // Directly: a call left for later is dropped once the widget is gone, and its claims would stay.
    Component.onDestruction: {
        if (holdsTopic) {
            Data.release(topicName);
            holdsTopic = false;
        }
        if (holdsClock) {
            FrameClock.release();
            holdsClock = false;
        }
    }

    Item {
        id: area

        readonly property bool small: globe.frame.small
        readonly property var homeAnchor: globe.labelAnchors[globe.labelAnchors.length - 1]

        function labelRect(anchor, text, dx, baseline) {
            if (!anchor || !anchor.visible) {
                return null;
            }
            const w = labelLine.advanceWidth(text);
            return { x: Uplink.labelX(anchor.x, dx, w, width), y: anchor.y + baseline - labelLine.ascent, w: w, h: labelLine.height };
        }

        // Imperative rather than a binding, since each result starts from the previous one.
        function placeLabels() {
            const rects = [globe.hasHome ? labelRect(homeAnchor, root.homeText, Theme.d(root.homeDxPx), Theme.d(root.homeBaselinePx)) : null];
            for (let i = 0; i < root.endpoints.length; i++) {
                rects.push(labelRect(globe.labelAnchors[i], root.endpoints[i].label, Theme.d(root.labelDxPx), Theme.d(root.labelBaselinePx)));
            }
            root.labelsShown = Uplink.shownLabels(rects, root.labelsShown, Theme.d(root.labelMarginPx));
        }

        anchors.fill: parent
        // Only a safety net: the labels turn inward and the footer elides before they reach the edge.
        clip: true
        Component.onCompleted: placeLabels()

        Globe {
            id: globe

            anchors.fill: parent
            elapsedMs: FrameClock.elapsedMs
            unit: Theme.u
            hasHome: Uplink.hasHome(Data.config)
            homeLat: hasHome ? root.location.lat : 0
            homeLon: hasHome ? root.location.lon : 0
            endpoints: root.endpoints
            signalColor: root.live ? Theme.signal : Theme.dim
            hiColor: root.live ? Theme.hi : Theme.dim

            // The anchors follow every frame of the turn and every change of the endpoints or the size.
            onLabelAnchorsChanged: area.placeLabels()
        }

        FontMetrics {
            id: labelLine

            font {
                family: Theme.font
                pixelSize: Theme.d(area.small ? root.smallLabelPx : root.labelPx)
                weight: Font.DemiBold
            }
        }

        Repeater {
            // A count rather than the endpoints, so a label keeps its object through the data's updates.
            model: root.endpoints.length

            Text {
                required property int index
                // A message that drops an endpoint may reach this binding before the Repeater drops the label.
                readonly property var anchor: globe.labelAnchors[index]
                readonly property var endpoint: root.endpoints[index]

                visible: anchor !== undefined && endpoint !== undefined && anchor.visible && root.labelsShown[index + 1] === true
                x: anchor ? Uplink.labelX(anchor.x, Theme.d(root.labelDxPx), implicitWidth, area.width) : 0
                y: anchor ? anchor.y + Theme.d(root.labelBaselinePx) - baselineOffset : 0
                text: endpoint ? endpoint.label : ""
                // Organisation, process and tailnet host names come from outside the shell.
                textFormat: Text.PlainText
                color: root.labelColor
                font: labelLine.font
            }
        }

        Text {
            visible: globe.hasHome && area.homeAnchor.visible
            x: Uplink.labelX(area.homeAnchor.x, Theme.d(root.homeDxPx), implicitWidth, area.width)
            y: area.homeAnchor.y + Theme.d(root.homeBaselinePx) - baselineOffset
            text: root.homeText
            textFormat: Text.PlainText
            color: root.live ? Theme.signal : Theme.dim
            font: labelLine.font
        }

        Text {
            visible: !area.small
            x: Theme.d(root.footerInsetPx)
            y: area.height - Theme.d(root.footerInsetPx) - baselineOffset
            width: area.width - 2 * Theme.d(root.footerInsetPx)
            elide: Text.ElideRight
            text: Uplink.footerText(root.uplink)
            textFormat: Text.PlainText
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(root.footerPx)
                weight: Font.DemiBold
            }
        }
    }
}
