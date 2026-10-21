import QtQuick
import GhostKit

// The album art in a hair border, or the mockup's rings while there is none.
Item {
    id: art

    // Design pixels, from the reference mockup's .art.
    readonly property real borderPx: 1
    // The mockup's .art backdrop, rgba(5, 8, 9, .6), and its rings: a radial gradient whose stops are fractions of
    // the distance from the centre to a corner, each ring in the signal colour at the given opacity.
    readonly property color backdrop: "#99050809"
    readonly property var rings: [
        {
            inner: 0.14,
            outer: 0.165,
            alpha: 1
        },
        {
            inner: 0.28,
            outer: 0.30,
            alpha: 0.55
        },
        {
            inner: 0.40,
            outer: 0.42,
            alpha: 1
        }
    ]

    required property real sidePx
    property string source
    // The bar's icon style is mono, which tints the art to the palette.
    property bool mono: true
    // A remembered track's art, which is shown dimmed like its words.
    property bool dimmed: false

    readonly property real border: Theme.d(borderPx)
    readonly property real inner: width - 2 * border
    // The mockup's gradient is sized to the farthest corner, so its stops are fractions of half the diagonal.
    readonly property real reach: inner / Math.SQRT2
    readonly property bool showsCover: cover.status === Image.Ready
    readonly property bool tinted: mono || dimmed

    width: Theme.d(sidePx)
    height: width

    Rectangle {
        anchors.fill: parent
        color: art.backdrop
    }

    Repeater {
        model: art.showsCover ? [] : art.rings

        Rectangle {
            required property var modelData

            anchors.centerIn: parent
            width: 2 * modelData.outer * art.reach
            height: width
            radius: width / 2
            color: "transparent"
            border {
                width: (modelData.outer - modelData.inner) * art.reach
                color: Qt.rgba(Theme.signal.r, Theme.signal.g, Theme.signal.b, modelData.alpha)
            }
        }
    }

    // Shown as it is in the colour icon style; the Duotone below draws it when it is tinted.
    Image {
        id: cover

        x: art.border
        y: art.border
        width: art.inner
        height: art.inner
        source: art.source
        sourceSize: Qt.size(art.inner, art.inner)
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        visible: art.showsCover && !art.tinted
    }

    Duotone {
        anchors.fill: cover
        visible: art.showsCover && art.tinted
        source: cover
        lightColor: art.dimmed ? Theme.dim : Theme.signal
        // The cover's texture holds the whole picture, of which the Image shows only the cropped middle.
        uvScale: cover.paintedWidth > 0 && cover.paintedHeight > 0 ? Qt.vector2d(cover.width / cover.paintedWidth, cover.height / cover.paintedHeight) : Qt.vector2d(1, 1)
    }

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border {
            width: art.border
            color: Theme.hair
        }
    }
}
