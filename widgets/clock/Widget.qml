import QtQuick
import GhostKit

// The time with its date line and uptime, in three styles: a frameless hero, a headerless band and a compact
// clock for the MONITOR band. The hero and the band add the weather and the most urgent INMET alert, which the
// clock wants for itself so that a layout need not include a weather widget.
Panel {
    id: root

    readonly property string heroStyle: "hero"
    readonly property string bandStyle: "band"
    readonly property string compactStyle: "compact"

    // Design pixels, from the reference mockup's .timehero, .timeband and .band-clock.
    // The big digits' line box is this share of their size; the mockup's .huge sets .9 and .big sets 1.
    readonly property real bigLineHeight: 0.9
    readonly property real compactLineHeight: 1
    readonly property real heroGapPx: 14
    readonly property real heroTimePx: 118
    readonly property real heroSecondsPx: 34
    readonly property real heroSecondsGapPx: 10
    readonly property real weatherGapPx: 16
    readonly property real bandSidePx: 16
    readonly property real bandColumnGapPx: 22
    readonly property real bandStackGapPx: 9
    readonly property real bandTimePx: 74
    readonly property real bandSecondsPx: 24
    readonly property real bandSecondsGapPx: 6
    readonly property real bandNowGapPx: 12
    readonly property real compactSidePx: 14
    readonly property real compactGapPx: 6
    readonly property real compactTimePx: 44
    readonly property real compactSecondsPx: 16
    readonly property real compactSecondsGapPx: 4
    readonly property real readingsGapPx: 10
    readonly property real dateLinePx: 11
    readonly property real dateLineGapPx: 18
    readonly property real tempPx: 26
    readonly property real unitPx: 13
    readonly property real conditionGapPx: 2
    // Letter spacing in em.
    readonly property real heroTimeTracking: -0.02
    readonly property real bandTimeTracking: -0.02
    readonly property real compactTimeTracking: -0.01
    readonly property real heroSecondsTracking: 0
    readonly property real heroDateTracking: 0.26
    readonly property real bandDateTracking: 0.2

    // The mockup's .kv grid has four columns for its three readings.
    readonly property int readingsTracks: 4
    readonly property real millisPerSecond: 1000

    property var settings

    readonly property string style: settings.style
    readonly property bool wantsWeather: settings.weather === true && style !== compactStyle

    readonly property double nowS: FrameClock.epochSecond
    readonly property real nowMs: nowS * millisPerSecond
    readonly property var clockDate: new Date(nowMs)
    readonly property var parts: Fmt.clockParts(clockDate)
    // The machine's own offset, as the date and the weather hours are read in local time.
    readonly property int offsetMin: -clockDate.getTimezoneOffset()

    readonly property var host: Data.topic("host").value
    readonly property real uptimeS: Fmt.uptimeSince(host ? host.boot_time_s : null, nowS)
    readonly property bool uptimeKnown: Number.isFinite(uptimeS)
    readonly property string uptimeText: Fmt.uptime(uptimeS)
    readonly property string dowText: Fmt.dow(clockDate)
    readonly property string dateText: Fmt.ddmmyy(clockDate)
    readonly property string cityText: (Data.config?.location?.city ?? "").toUpperCase()
    readonly property string utcText: Fmt.utcOffset(offsetMin)

    readonly property var weather: Data.topic("weather").value

    // The daemon publishes the unavailable default until a forecast for a configured place has arrived.
    readonly property bool available: wantsWeather && weather !== null && weather.available === true
    // The daemon's own verdict on the forecast's age; a reconnect re-delivering the last message must not
    // make it look fresh.
    readonly property bool weatherLive: available && Data.topic("weather").live && !weather.stale
    readonly property var forecast: Fmt.forecastFrom(weather, nowS)
    readonly property string tempText: available && Number.isFinite(weather.temp_c) ? Math.round(weather.temp_c).toString() : "--"
    readonly property string conditionText: available ? weather.condition : ""
    readonly property string hiLoHumText: available ? Fmt.hiLoHum(weather) : ""
    readonly property string hiLoText: available ? Fmt.hiLo(weather) : ""
    readonly property string detailText: available ? Fmt.detailLine(forecast, offsetMin) : ""

    property bool holdsSeconds: false
    property bool holdsWeather: false

    // The seconds, the countdown and an alert's expiry all read the wall clock, which the frame clock keeps
    // current only for a holder.
    function holdSeconds(wanted) {
        if (wanted === holdsSeconds) {
            return;
        }
        if (wanted) {
            FrameClock.acquireSeconds();
        } else {
            FrameClock.releaseSeconds();
        }
        holdsSeconds = wanted;
    }

    // Once per widget rather than per tick: every change of the subscription makes the daemon resend history
    // and replay each value.
    function holdWeather(wanted) {
        if (wanted === holdsWeather) {
            return;
        }
        if (wanted) {
            Data.want("weather");
            Data.want("weather.alerts");
        } else {
            Data.release("weather");
            Data.release("weather.alerts");
        }
        holdsWeather = wanted;
    }

    // The big digits, with the seconds beside them. The seconds' line box sits at the top of the digits' own
    // (the mockup's vertical-align: top), and neither is ever scrambled.
    component Stamp: Item {
        id: stamp

        property string hours
        property string minutes
        property string seconds
        required property real timePx
        required property real secondsPx
        required property real secondsGapPx
        required property real lineHeight
        required property real timeTracking
        // The mockup's .ss inherits the digits' letter spacing, except in the hero.
        property real secondsTracking: stamp.timeTracking

        implicitWidth: time.implicitWidth + Theme.d(secondsGapPx) + secondsText.implicitWidth
        implicitHeight: Theme.d(timePx) * lineHeight

        Digits {
            id: time

            anchors.verticalCenter: parent.verticalCenter
            text: stamp.hours + ":" + stamp.minutes
            pixelSize: Theme.d(stamp.timePx)
            tracking: stamp.timeTracking
            color: Theme.hi
        }

        Digits {
            id: secondsText

            x: Math.round(time.implicitWidth + Theme.d(stamp.secondsGapPx))
            y: Math.round((Theme.d(stamp.secondsPx) * stamp.lineHeight - implicitHeight) / 2)
            text: stamp.seconds
            pixelSize: Theme.d(stamp.secondsPx)
            tracking: stamp.secondsTracking
            color: Theme.signal
        }
    }

    // A dim label with its value beside it, one space apart as in running text.
    component Segment: Row {
        id: segment

        property string label
        property string value
        property color valueColor: Theme.text
        required property real textPx
        required property real tracking

        spacing: space.advanceWidth

        ScrambleText {
            id: labelText

            text: segment.label
            textFormat: Text.PlainText
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(segment.textPx)
                weight: Font.Medium
                letterSpacing: Theme.d(segment.textPx) * segment.tracking
            }
        }

        Digits {
            text: segment.value
            pixelSize: Theme.d(segment.textPx)
            weight: Font.Medium
            tracking: segment.tracking
            color: segment.valueColor
        }

        TextMetrics {
            id: space

            font: labelText.font
            text: " "
        }
    }

    // The weekday, the date, the uptime and, when a city is set, the city with the machine's UTC offset.
    component DateLine: Row {
        id: line

        property string dow
        property string date
        property string uptime
        property color uptimeColor: Theme.text
        property string city
        property string offset
        required property real textPx
        required property real tracking
        required property real gapPx

        spacing: Theme.d(gapPx)

        Digits {
            text: line.dow
            pixelSize: Theme.d(line.textPx)
            weight: Font.Medium
            tracking: line.tracking
            color: Theme.text
        }

        Segment {
            label: "DATE"
            value: line.date
            textPx: line.textPx
            tracking: line.tracking
        }

        Segment {
            label: "UPTIME"
            value: line.uptime
            valueColor: line.uptimeColor
            textPx: line.textPx
            tracking: line.tracking
        }

        Segment {
            visible: line.city !== ""
            label: line.city
            value: line.offset
            textPx: line.textPx
            tracking: line.tracking
        }
    }

    // The temperature with its °C raised as a superscript.
    component TempReading: Item {
        id: reading

        // Design pixels, from the mockup's .temp sup.
        readonly property real unitGapPx: 2
        // The mockup's °C has its top level with the digits' top, so it is lifted by how far the digits' cap
        // height exceeds its own.
        readonly property real capHeightEm: 0.7

        property string text
        property bool sourceLive: true
        required property real tempPx
        required property real unitPx

        implicitWidth: unit.x + unit.width
        // The mockup's .temp has a line height of 1, which is less than the digits' own.
        implicitHeight: Theme.d(tempPx)

        Digits {
            id: temp

            anchors.verticalCenter: parent.verticalCenter
            text: reading.text
            pixelSize: Theme.d(reading.tempPx)
            sourceLive: reading.sourceLive
        }

        ScrambleText {
            id: unit

            anchors {
                left: temp.right
                leftMargin: Theme.d(reading.unitGapPx)
                baseline: temp.baseline
                baselineOffset: -Theme.d((reading.tempPx - reading.unitPx) * reading.capHeightEm)
            }
            text: "°C"
            color: Theme.dim
            font {
                family: Theme.font
                pixelSize: Theme.d(reading.unitPx)
                weight: Font.Medium
            }
        }
    }

    // Design pixels, from the mockup's .wx-cond and .wxs.
    component Condition: ScrambleText {
        readonly property real textPx: 10
        // Letter spacing in em.
        readonly property real tracking: 0.12

        property bool live: true

        // The condition is the daemon's word for a weather code, which AutoText would render as markup.
        textFormat: Text.PlainText
        color: live ? Theme.hi : Theme.dim
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(textPx)
            weight: Font.Medium
            letterSpacing: Theme.d(textPx) * tracking
        }
    }

    component DimLine: ScrambleText {
        readonly property real textPx: 10

        textFormat: Text.PlainText
        color: Theme.dim
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(textPx)
            weight: Font.Medium
        }
    }

    anchors.fill: parent
    header: false
    framed: style !== heroStyle
    padding: 0

    onVisibleChanged: holdSeconds(visible)
    onWantsWeatherChanged: holdWeather(wantsWeather)
    Component.onCompleted: {
        holdSeconds(visible);
        holdWeather(wantsWeather);
    }
    Component.onDestruction: {
        holdSeconds(false);
        holdWeather(false);
    }

    WeatherAlertState {
        id: alertState

        nowS: root.nowS
        // Not root.available: the alerts come from their own provider and must not wait on the forecast.
        enabled: root.wantsWeather
    }

    Component {
        id: hero

        Item {
            Column {
                id: stack

                anchors.centerIn: parent
                spacing: Theme.d(root.heroGapPx)

                Stamp {
                    anchors.horizontalCenter: parent.horizontalCenter
                    hours: root.parts.hours
                    minutes: root.parts.minutes
                    seconds: root.parts.seconds
                    timePx: root.heroTimePx
                    secondsPx: root.heroSecondsPx
                    secondsGapPx: root.heroSecondsGapPx
                    lineHeight: root.bigLineHeight
                    timeTracking: root.heroTimeTracking
                    secondsTracking: root.heroSecondsTracking
                }

                DateLine {
                    anchors.horizontalCenter: parent.horizontalCenter
                    dow: root.dowText
                    date: root.dateText
                    uptime: root.uptimeText
                    uptimeColor: root.uptimeKnown ? Theme.text : Theme.dim
                    city: root.cityText
                    offset: root.utcText
                    textPx: root.dateLinePx
                    tracking: root.heroDateTracking
                    gapPx: root.dateLineGapPx
                }

                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: root.available
                    spacing: Theme.d(root.weatherGapPx)

                    TempReading {
                        id: heroTemp

                        anchors.verticalCenter: parent.verticalCenter
                        text: root.tempText
                        sourceLive: root.weatherLive
                        tempPx: root.tempPx
                        unitPx: root.unitPx
                    }

                    // A long condition gives way, so the line never grows past the panel.
                    Condition {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, Math.max(0, root.width - heroTemp.width - heroHiLoHum.width - 2 * parent.spacing))
                        text: root.conditionText
                        live: root.weatherLive
                    }

                    DimLine {
                        id: heroHiLoHum

                        anchors.verticalCenter: parent.verticalCenter
                        text: root.hiLoHumText
                    }
                }

                // Keyed on the alert, so a different alert arrives with its own flicker while the countdown of
                // the same one does not restart it.
                Repeater {
                    model: alertState.hasAlert ? [alertState.key] : []

                    // The hero has the room for the source, as in the mockup.
                    AlertBox {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.min(implicitWidth, root.width)
                        level: alertState.level
                        chipKey: "alert"
                        alertKey: alertState.key
                        text: Fmt.alertLine(alertState.top, alertState.source)
                        trailing: alertState.countdown
                        sourceLive: alertState.live
                    }
                }
            }
        }
    }

    Component {
        id: band

        Item {
            Item {
                anchors {
                    fill: parent
                    leftMargin: Theme.d(root.bandSidePx)
                    rightMargin: Theme.d(root.bandSidePx)
                }

                Stamp {
                    id: bandStamp

                    anchors.verticalCenter: parent.verticalCenter
                    hours: root.parts.hours
                    minutes: root.parts.minutes
                    seconds: root.parts.seconds
                    timePx: root.bandTimePx
                    secondsPx: root.bandSecondsPx
                    secondsGapPx: root.bandSecondsGapPx
                    lineHeight: root.bigLineHeight
                    timeTracking: root.bandTimeTracking
                }

                Column {
                    id: details

                    anchors {
                        left: bandStamp.right
                        leftMargin: Theme.d(root.bandColumnGapPx)
                        right: parent.right
                        verticalCenter: parent.verticalCenter
                    }
                    spacing: Theme.d(root.bandStackGapPx)

                    DateLine {
                        dow: root.dowText
                        date: root.dateText
                        uptime: root.uptimeText
                        uptimeColor: root.uptimeKnown ? Theme.text : Theme.dim
                        textPx: root.dateLinePx
                        tracking: root.bandDateTracking
                        gapPx: root.dateLineGapPx
                    }

                    Item {
                        width: details.width
                        height: Math.max(bandTemp.height, bandCondition.height)
                        visible: root.available

                        TempReading {
                            id: bandTemp

                            anchors.verticalCenter: parent.verticalCenter
                            text: root.tempText
                            sourceLive: root.weatherLive
                            tempPx: root.tempPx
                            unitPx: root.unitPx
                        }

                        Column {
                            id: bandCondition

                            anchors {
                                left: bandTemp.right
                                leftMargin: Theme.d(root.bandNowGapPx)
                                right: parent.right
                                verticalCenter: parent.verticalCenter
                            }
                            spacing: Theme.d(root.conditionGapPx)

                            // The high and low share the condition's line, since the line of humidity, wind and
                            // rain that follows is the one that needs the whole width.
                            Row {
                                width: parent.width
                                spacing: Theme.d(root.weatherGapPx)

                                Condition {
                                    width: Math.min(implicitWidth, Math.max(0, parent.width - hiLoLine.width - parent.spacing))
                                    text: root.conditionText
                                    live: root.weatherLive
                                }

                                DimLine {
                                    id: hiLoLine

                                    text: root.hiLoText
                                }
                            }

                            DimLine {
                                width: parent.width
                                text: root.detailText
                            }
                        }
                    }

                    // The source is left out: with it the severity, which sits at the end, would be the part cut in a
                    // column this narrow, and a config names one provider anyway (the weather band does the same).
                    Repeater {
                        model: alertState.hasAlert ? [alertState.key] : []

                        AlertBox {
                            width: details.width
                            level: alertState.level
                            chipKey: "alert"
                            alertKey: alertState.key
                            text: Fmt.alertLine(alertState.top, "")
                            trailing: alertState.countdown
                            sourceLive: alertState.live
                        }
                    }
                }
            }
        }
    }

    Component {
        id: compact

        Item {
            Column {
                id: compactStack

                anchors {
                    left: parent.left
                    leftMargin: Theme.d(root.compactSidePx)
                    right: parent.right
                    rightMargin: Theme.d(root.compactSidePx)
                    verticalCenter: parent.verticalCenter
                }
                spacing: Theme.d(root.compactGapPx)

                Stamp {
                    hours: root.parts.hours
                    minutes: root.parts.minutes
                    seconds: root.parts.seconds
                    timePx: root.compactTimePx
                    secondsPx: root.compactSecondsPx
                    secondsGapPx: root.compactSecondsGapPx
                    lineHeight: root.compactLineHeight
                    timeTracking: root.compactTimeTracking
                }

                // The mockup lays three readings out in a four-column grid that spreads its tracks apart, so
                // the readings sit at the left, a third and two thirds of the way, and the last track stays empty.
                Row {
                    id: readings

                    readonly property real gap: Theme.d(root.readingsGapPx)
                    readonly property real slack: compactStack.width - dateReading.width - dayReading.width - uptimeReading.width - (root.readingsTracks - 1) * gap

                    spacing: gap + Math.max(0, slack) / (root.readingsTracks - 1)

                    Kv {
                        id: dateReading

                        key: "DATE"
                        value: root.dateText
                        valueColor: Theme.hi
                    }

                    Kv {
                        id: dayReading

                        key: "DAY"
                        value: root.dowText
                        valueColor: Theme.hi
                    }

                    Kv {
                        id: uptimeReading

                        key: "UPTIME"
                        value: root.uptimeText
                        valueColor: root.uptimeKnown ? Theme.hi : Theme.dim
                    }
                }
            }
        }
    }

    Loader {
        anchors.fill: parent
        sourceComponent: root.style === root.heroStyle ? hero : root.style === root.bandStyle ? band : compact
    }
}
