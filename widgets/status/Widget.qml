import QtQuick
import GhostKit

// A compact readout of the machine in the bar: CPU, GPU and memory load, traffic, the VPN count and the weather.
Item {
    id: root

    anchors.fill: parent

    // Design pixels, from the reference mockup's .st.
    readonly property real partGapPx: 14
    readonly property real labelGapPx: 5
    readonly property real chevronPx: 8
    readonly property real textPx: 10
    // Letter spacing in em.
    readonly property real labelTracking: 0.12
    readonly property real percent: 100
    // The order the parts are drawn in, whatever order the setting lists them.
    readonly property var order: ["cpu", "gpu", "mem", "net", "vpn", "weather"]
    readonly property var topicsOf: ({
            cpu: ["cpu"],
            gpu: ["gpu"],
            mem: ["mem"],
            net: ["net"],
            vpn: ["net.links"],
            weather: ["weather", "weather.alerts"]
        })

    property var settings

    readonly property var shown: order.filter(part => (settings.parts ?? []).indexOf(part) >= 0)
    readonly property var wantedTopics: visible ? shown.reduce((topics, part) => topics.concat(topicsOf[part]), []) : []
    property var heldTopics: []

    readonly property var cpu: Data.topic("cpu").value
    readonly property var gpu: Data.topic("gpu").value
    readonly property var mem: Data.topic("mem").value
    readonly property var net: Data.topic("net").value
    readonly property var links: Data.topic("net.links").value
    readonly property var weather: Data.topic("weather").value
    readonly property var alerts: Data.topic("weather.alerts").value
    readonly property double nowS: FrameClock.epochSecond

    readonly property real cpuLoad: cpu && cpu.threads.length > 0 ? cpu.load : NaN
    readonly property real gpuLoad: gpu && gpu.present ? gpu.load : NaN
    readonly property real memShare: mem && mem.total_kib > 0 ? mem.used_kib / mem.total_kib * percent : NaN
    readonly property int vpnsUp: links ? links.vpns.filter(vpn => vpn.state === "up").length : -1
    readonly property var activeAlert: Fmt.activeAlerts(alerts ? alerts.alerts : null, nowS)[0] ?? null
    readonly property bool weatherKnown: weather !== null && weather.available === true && Number.isFinite(weather.temp_c)

    implicitWidth: row.implicitWidth

    // Claims only what changed, since every change of the subscription makes the daemon replay each topic.
    function syncTopics() {
        const wanted = wantedTopics;
        const held = heldTopics.slice();
        held.filter(topic => wanted.indexOf(topic) < 0).forEach(topic => {
            Data.release(topic);
            held.splice(held.indexOf(topic), 1);
        });
        wanted.filter(topic => held.indexOf(topic) < 0).forEach(topic => {
            Data.want(topic);
            held.push(topic);
        });
        heldTopics = held;
    }

    function has(part) {
        return shown.indexOf(part) >= 0;
    }

    function levelColor(level) {
        return level === "crit" ? Theme.crit : level === "warn" ? Theme.warn : Theme.text;
    }

    function loadColor(loadPct) {
        return levelColor(Fmt.levelAbove(loadPct / percent, Theme.loadWarn, Theme.loadCrit));
    }

    onWantedTopicsChanged: syncTopics()
    Component.onCompleted: syncTopics()
    Component.onDestruction: {
        heldTopics.forEach(topic => Data.release(topic));
        heldTopics = [];
    }

    component Label: Text {
        color: Theme.dim
        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
            letterSpacing: Theme.d(root.textPx) * root.labelTracking
        }
    }

    component Value: Text {
        color: Theme.text
        font {
            family: Theme.font
            pixelSize: Theme.d(root.textPx)
        }
    }

    component Part: Row {
        required property string label
        required property string value
        property color valueColor: Theme.text

        spacing: Theme.d(root.labelGapPx)

        Label {
            text: parent.label
        }

        Value {
            text: parent.value
            color: parent.valueColor
        }
    }

    component Chevron: Glyph {
        size: Theme.d(root.chevronPx)
        color: Theme.dim
    }

    // The parts keep this order whatever the setting lists, and a part the setting leaves out takes no room.
    Row {
        id: row

        height: parent.height
        spacing: Theme.d(root.partGapPx)

        Part {
            visible: root.has("cpu")
            anchors.verticalCenter: parent.verticalCenter
            label: "CPU"
            value: Fmt.pct(root.cpuLoad)
            valueColor: root.loadColor(root.cpuLoad)
        }

        Part {
            visible: root.has("gpu")
            anchors.verticalCenter: parent.verticalCenter
            label: "GPU"
            value: Fmt.pct(root.gpuLoad)
            valueColor: root.loadColor(root.gpuLoad)
        }

        Part {
            visible: root.has("mem")
            anchors.verticalCenter: parent.verticalCenter
            label: "MEM"
            value: Fmt.memShort(root.mem ? root.mem.used_kib : NaN, root.mem ? root.mem.total_kib : NaN)
            valueColor: root.loadColor(root.memShare)
        }

        Row {
            visible: root.has("net")
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.d(root.labelGapPx)

            Chevron {
                anchors.verticalCenter: parent.verticalCenter
                name: "chevron-up"
            }

            Value {
                text: Fmt.rateShort(root.net ? root.net.tx_bps : NaN)
            }

            Chevron {
                anchors.verticalCenter: parent.verticalCenter
                name: "chevron-down"
            }

            Value {
                text: Fmt.rateShort(root.net ? root.net.rx_bps : NaN)
            }
        }

        Part {
            visible: root.has("vpn")
            anchors.verticalCenter: parent.verticalCenter
            label: "VPN"
            value: root.vpnsUp < 0 ? Fmt.pct(NaN) : root.vpnsUp + " UP"
        }

        Row {
            visible: root.has("weather")
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.d(root.labelGapPx)

            Label {
                text: "WX"
            }

            Value {
                text: root.weatherKnown ? Fmt.tempShort(root.weather.temp_c) : Fmt.tempShort(NaN)
            }

            Value {
                visible: root.activeAlert !== null
                text: root.activeAlert ? "· " + root.activeAlert.title : ""
                color: Theme.warn
            }
        }
    }
}
