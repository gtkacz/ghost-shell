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

    property var settings

    readonly property string topicName: "uplink"
    readonly property var uplink: Data.topic(topicName).value
    readonly property var endpoints: uplink ? uplink.endpoints : []
    readonly property var location: Data.config ? Data.config.location : null
    readonly property bool wantsGlobe: visible && !Data.covered

    property bool holdsGlobe: false

    // Once per change rather than per frame: every change of the subscription makes the daemon replay each topic.
    function holdGlobe() {
        if (wantsGlobe === holdsGlobe) {
            return;
        }
        if (wantsGlobe) {
            Data.want(topicName);
            FrameClock.acquire();
        } else {
            Data.release(topicName);
            FrameClock.release();
        }
        holdsGlobe = wantsGlobe;
    }

    anchors.fill: parent
    label: "Uplink"
    tagKey: "scope"

    // A hold changes a turn later, never inside the update of the binding that asked for it.
    onWantsGlobeChanged: Qt.callLater(holdGlobe)
    Component.onCompleted: holdGlobe()
    // Directly: a call left for later is dropped once the widget is gone, and its claims would stay.
    Component.onDestruction: {
        if (holdsGlobe) {
            Data.release(topicName);
            FrameClock.release();
            holdsGlobe = false;
        }
    }

    Item {
        id: area

        readonly property bool small: globe.frame.small
        readonly property var homeAnchor: globe.labelAnchors[globe.labelAnchors.length - 1]

        anchors.fill: parent
        // A label beside an endpoint on the right limb runs past the body, where the mockup's canvas ends it.
        clip: true

        Globe {
            id: globe

            anchors.fill: parent
            elapsedMs: FrameClock.elapsedMs
            unit: Theme.u
            hasHome: Uplink.hasHome(Data.config)
            homeLat: hasHome ? root.location.lat : 0
            homeLon: hasHome ? root.location.lon : 0
            endpoints: root.endpoints
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
            model: Math.min(root.endpoints.length, globe.labelAnchors.length - 1)

            Text {
                required property int index
                // A message that drops an endpoint may reach this binding before the Repeater drops the label.
                readonly property var anchor: globe.labelAnchors[index]
                readonly property var endpoint: root.endpoints[index]

                visible: anchor !== undefined && endpoint !== undefined && anchor.visible
                x: anchor ? anchor.x + Theme.d(root.labelDxPx) : 0
                y: anchor ? anchor.y + Theme.d(root.labelBaselinePx) - baselineOffset : 0
                text: endpoint ? endpoint.label : ""
                // Organisation, process and tailnet host names come from outside the shell.
                textFormat: Text.PlainText
                color: Qt.rgba(Theme.hi.r, Theme.hi.g, Theme.hi.b, root.labelOpacity)
                font: labelLine.font
            }
        }

        Text {
            visible: globe.hasHome && area.homeAnchor.visible
            x: area.homeAnchor.x + Theme.d(root.homeDxPx)
            y: area.homeAnchor.y + Theme.d(root.homeBaselinePx) - baselineOffset
            text: Uplink.homeLabel(Data.topic("host").value)
            textFormat: Text.PlainText
            color: Theme.signal
            font: labelLine.font
        }

        Text {
            visible: !area.small
            x: Theme.d(root.footerInsetPx)
            y: area.height - Theme.d(root.footerInsetPx) - baselineOffset
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
