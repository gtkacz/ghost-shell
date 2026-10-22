import QtQuick

// The most urgent weather alert still in force, read from the weather.alerts topic, as the clock and the weather
// widget both show it. Every output is a scalar except the alert itself, so a keyed Repeater on hasAlert and key is
// rebuilt when the alert changes and not when its countdown ticks. It only reads the topic: the widget using it
// must subscribe, through its manifest's requires or Data.want, or it never has an alert.
QtObject {
    id: root

    readonly property real millisPerSecond: 1000

    // Seconds since the epoch, from FrameClock.epochSecond, which only runs for a holder of acquireSeconds().
    // Alerts expire against it.
    property double nowS
    // Lets a widget leave the alert out where it shows no weather; never tie it to the forecast being available.
    property bool enabled: true

    readonly property var value: Data.topic("weather.alerts").value
    // With no provider configured the topic has nothing to say, so no alert is even considered.
    readonly property var active: enabled && Fmt.alertsEnabled(Data.config) && value ? Fmt.activeAlerts(value.alerts, nowS) : []

    // Crit first, then the earliest start. An alert that has not started is kept, for its countdown to the start.
    readonly property var top: active.length > 0 ? active[0] : null
    readonly property bool hasAlert: top !== null
    readonly property string key: hasAlert ? Fmt.alertKey(top) : ""
    readonly property string level: hasAlert ? top.level : "warn"
    readonly property string source: hasAlert && value.source ? value.source : ""
    readonly property string countdown: hasAlert ? Fmt.countdown(nowS * millisPerSecond, top.start_s * millisPerSecond, top.end_s * millisPerSecond) : ""
    // A provider that stopped publishing leaves its last alert up, so the alert dims once the provider is
    // gone or its data too old to trust.
    readonly property bool live: Data.topic("weather.alerts").live && !Fmt.alertsStale(value, nowS)
}
