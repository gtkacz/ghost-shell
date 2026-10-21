import QtQuick
import GhostKit

// The weather band: the temperature and condition, the most urgent alert with the day's detail, sun, moon and
// air quality, and the next hours' chance of rain. It shows nothing until the daemon has a forecast, which it
// has only once a location is set.
Panel {
    id: root

    // Design pixels, from the reference mockup's .band-wx.
    readonly property real paddingPx: 10
    // The band's sides are 14 px against the panel's own 10.
    readonly property real sideInsetPx: 4
    readonly property real columnGapPx: 16
    readonly property real hoursWidthPx: 140
    readonly property real nowGapPx: 12
    readonly property real tempPx: 34
    readonly property real unitPx: 13
    readonly property real unitGapPx: 2
    // The mockup's °C is a superscript with its top level with the digits' top, so it is lifted by how far the
    // digits' cap height exceeds its own.
    readonly property real capHeightEm: 0.7
    readonly property real conditionPx: 10
    readonly property real conditionGapPx: 2
    // Letter spacing in em.
    readonly property real conditionTracking: 0.12
    readonly property int conditionLines: 2
    readonly property string conditionCapText: "THUNDERSTORM"
    readonly property real stackGapPx: 6
    readonly property real detailPx: 9
    readonly property real aqiGapPx: 8
    readonly property real hoursGapPx: 6
    readonly property real hourPx: 8.5
    readonly property real hourStackGapPx: 2
    readonly property real rainWidthShare: 0.7
    readonly property real rainHeightPx: 14
    readonly property real rainCellPx: 2
    readonly property real rainGapPx: 1
    readonly property int hourColumns: 6
    readonly property real millisPerSecond: 1000

    property var settings

    readonly property var weather: Data.topic("weather").value
    readonly property var sky: Data.topic("sky").value

    // The daemon publishes the unavailable default until a forecast for a configured place has arrived.
    readonly property bool available: weather !== null && weather.available === true
    // The daemon's own verdict on the forecast's age; a reconnect re-delivering the last message must not
    // make it look fresh.
    readonly property bool weatherLive: available && Data.topic("weather").live && !weather.stale

    readonly property double nowS: FrameClock.epochSecond
    readonly property real nowMs: nowS * millisPerSecond
    // The machine's own offset, as the sun times and the forecast hours are read in local time.
    readonly property int offsetMin: -new Date(nowMs).getTimezoneOffset()
    readonly property var forecast: Fmt.forecastFrom(weather, nowS)
    readonly property var hours: forecast ? forecast.hours : []

    readonly property string tempText: available && Number.isFinite(weather.temp_c) ? Math.round(weather.temp_c).toString() : "--"
    readonly property string conditionText: available ? weather.condition : ""
    readonly property string hiLoText: available ? Fmt.hiLo(weather) : ""
    readonly property string detailText: available ? Fmt.detailLine(forecast, offsetMin) : ""
    readonly property string skyText: sky && sky.available ? Fmt.skyLine(sky, offsetMin) : ""
    readonly property var aqi: available ? weather.aqi : null
    readonly property bool hasAqi: Number.isFinite(aqi)
    readonly property string aqiLevel: Fmt.levelAbove(aqi, Theme.aqiWarn, Theme.aqiCrit)
    readonly property color aqiColor: !weatherLive ? Theme.dim : aqiLevel === "crit" ? Theme.crit : aqiLevel === "warn" ? Theme.warn : Theme.dim

    // The source is left out: a config names one provider, the chip and the level colour already say "alert",
    // and the mockup's band shows the title alone. The room goes to the severity, which sits at the end.
    readonly property string alertText: alertState.hasAlert ? Fmt.alertLine(alertState.top, "") : ""

    property bool holdsSeconds: false

    // The countdown, the alert's expiry and the hours that have passed all read the wall clock, which the
    // frame clock keeps current only for a holder.
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

    anchors.fill: parent
    visible: available
    header: false
    padding: Theme.d(paddingPx)

    onVisibleChanged: holdSeconds(visible)
    Component.onCompleted: holdSeconds(visible)
    Component.onDestruction: holdSeconds(false)

    WeatherAlertState {
        id: alertState

        nowS: root.nowS
    }

    FontMetrics {
        id: hourMetrics

        font {
            family: Theme.font
            pixelSize: Theme.d(root.hourPx)
            weight: Font.Medium
        }
    }

    Item {
        id: band

        anchors {
            fill: parent
            leftMargin: Theme.d(root.sideInsetPx)
            rightMargin: Theme.d(root.sideInsetPx)
        }

        Item {
            id: now

            readonly property real gap: Theme.d(root.nowGapPx)

            anchors {
                left: parent.left
                verticalCenter: parent.verticalCenter
            }
            width: unit.x + unit.width + gap + conditionColumn.width
            height: Math.max(temp.height, conditionColumn.height)

            Digits {
                id: temp

                anchors.verticalCenter: parent.verticalCenter
                text: root.tempText
                pixelSize: Theme.d(root.tempPx)
                sourceLive: root.weatherLive
            }

            Text {
                id: unit

                anchors {
                    left: temp.right
                    leftMargin: Theme.d(root.unitGapPx)
                    baseline: temp.baseline
                    baselineOffset: -Theme.d((root.tempPx - root.unitPx) * root.capHeightEm)
                }
                text: "°C"
                color: Theme.dim
                font {
                    family: Theme.font
                    pixelSize: Theme.d(root.unitPx)
                    weight: Font.Medium
                }
            }

            TextMetrics {
                id: conditionCap

                font: conditionLabel.font
                text: root.conditionCapText
            }

            TextMetrics {
                id: conditionWanted

                font: conditionLabel.font
                text: root.conditionText
            }

            Column {
                id: conditionColumn

                anchors {
                    left: unit.right
                    leftMargin: now.gap
                    verticalCenter: parent.verticalCenter
                }
                spacing: Theme.d(root.conditionGapPx)

                Text {
                    id: conditionLabel

                    // The mockup's own condition is the widest the column is given, so that a longer one wraps instead
                    // of widening this column and starving the alert and detail lines beside it.
                    width: Math.min(conditionWanted.advanceWidth, conditionCap.advanceWidth)
                    text: root.conditionText
                    // The condition is the daemon's word for a weather code, which AutoText would render as markup.
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    maximumLineCount: root.conditionLines
                    elide: Text.ElideRight
                    color: root.weatherLive ? Theme.hi : Theme.dim
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.conditionPx)
                        weight: Font.Medium
                        letterSpacing: Theme.d(root.conditionPx) * root.conditionTracking
                    }
                }

                Text {
                    text: root.hiLoText
                    textFormat: Text.PlainText
                    color: Theme.dim
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.conditionPx)
                        weight: Font.Medium
                    }
                }
            }
        }

        Column {
            id: middle

            anchors {
                left: now.right
                leftMargin: Theme.d(root.columnGapPx)
                right: hoursBlock.left
                rightMargin: Theme.d(root.columnGapPx)
                verticalCenter: parent.verticalCenter
            }
            spacing: Theme.d(root.stackGapPx)

            // Keyed on the alert, so a different alert arrives with its own flicker while the countdown of
            // the same one does not restart it.
            Repeater {
                model: alertState.hasAlert ? [alertState.key] : []

                AlertBox {
                    width: middle.width
                    level: alertState.level
                    chipKey: "alert"
                    alertKey: alertState.key
                    text: root.alertText
                    trailing: alertState.countdown
                    sourceLive: alertState.live
                }
            }

            Text {
                width: middle.width
                text: root.detailText
                textFormat: Text.PlainText
                color: Theme.dim
                elide: Text.ElideRight
                font {
                    family: Theme.font
                    pixelSize: Theme.d(root.detailPx)
                    weight: Font.Medium
                }
            }

            Item {
                width: middle.width
                height: skyLine.implicitHeight

                // The air quality keeps its place at the right edge and the sky line gives way to it, since a
                // moon name is the part that can be cut.
                Text {
                    id: skyLine

                    width: Math.min(implicitWidth, Math.max(0, parent.width - (root.hasAqi ? aqiText.width + Theme.d(root.aqiGapPx) : 0)))
                    text: root.skyText
                    textFormat: Text.PlainText
                    color: Theme.dim
                    elide: Text.ElideRight
                    font {
                        family: Theme.font
                        pixelSize: Theme.d(root.detailPx)
                        weight: Font.Medium
                    }
                }

                Text {
                    id: aqiText

                    anchors {
                        right: parent.right
                        baseline: skyLine.baseline
                    }
                    width: Math.min(implicitWidth, parent.width)
                    visible: root.hasAqi
                    text: "AQI " + root.aqi
                    textFormat: Text.PlainText
                    color: root.aqiColor
                    elide: Text.ElideRight
                    font: skyLine.font
                }
            }
        }

        Item {
            id: hoursBlock

            readonly property real gap: Theme.d(root.hoursGapPx)
            readonly property real cellWidth: (width - (root.hourColumns - 1) * gap) / root.hourColumns
            readonly property real stackGap: Theme.d(root.hourStackGapPx)

            anchors {
                right: parent.right
                verticalCenter: parent.verticalCenter
            }
            width: Theme.d(root.hoursWidthPx)
            height: Theme.d(root.rainHeightPx) + 2 * stackGap + 2 * Math.ceil(hourMetrics.height)

            Repeater {
                model: root.hours.length

                Column {
                    id: cell

                    required property int index
                    readonly property var hour: root.hours[index] ?? null
                    readonly property real rainPct: hour ? hour.rain_pct : NaN
                    readonly property real tempC: hour ? hour.temp_c : NaN
                    readonly property string label: hour ? Fmt.hourLabel(hour.at_s, root.offsetMin) : ""

                    x: index * (hoursBlock.cellWidth + hoursBlock.gap)
                    width: hoursBlock.cellWidth
                    spacing: hoursBlock.stackGap

                    BlockGraph {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: cell.width * root.rainWidthShare
                        height: Theme.d(root.rainHeightPx)
                        values: [cell.rainPct]
                        cols: 1
                        cellH: root.rainCellPx
                        gap: root.rainGapPx
                        // The chance of rain is not an alarm, so its cells stay signal at any height, and a
                        // chance too small to light a cell lights none.
                        warnAt: Infinity
                        critAt: Infinity
                        minLit: 0
                        sourceLive: root.weatherLive
                    }

                    Digits {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: Fmt.tempShort(cell.tempC)
                        pixelSize: Theme.d(root.hourPx)
                        weight: Font.Medium
                        color: root.weatherLive ? Theme.text : Theme.dim
                    }

                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: cell.label
                        textFormat: Text.PlainText
                        color: Theme.dim
                        font: hourMetrics.font
                    }
                }
            }
        }
    }
}
