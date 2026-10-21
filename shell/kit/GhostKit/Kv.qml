import QtQuick

// A labelled reading: the key in small dim capitals above its value.
Item {
    id: root

    // Design pixels.
    readonly property real keyPx: 8.5
    readonly property real valuePx: 10
    readonly property real spacingPx: 1
    // Letter spacing in em.
    readonly property real keyTracking: 0.14

    property string key
    property string value
    // Defaults to Digits' colour; a widget overrides it to dim a reading that is missing.
    property alias valueColor: valueText.color

    implicitWidth: Math.max(keyText.implicitWidth, valueText.implicitWidth)
    implicitHeight: valueText.y + valueText.implicitHeight

    Text {
        id: keyText

        width: root.width
        text: root.key
        color: Theme.dim
        elide: Text.ElideRight
        font {
            family: Theme.font
            pixelSize: Theme.d(root.keyPx)
            weight: Font.Medium
            letterSpacing: Theme.d(root.keyPx) * root.keyTracking
        }
    }

    Digits {
        id: valueText

        y: keyText.implicitHeight + Theme.d(root.spacingPx)
        width: root.width
        text: root.value
        pixelSize: Theme.d(root.valuePx)
        weight: Font.Medium
    }
}
