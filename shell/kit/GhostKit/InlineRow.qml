import QtQuick

// Lays out its children left to right as Row does, skipping the hidden ones and those with no width or height, but
// from a binding instead of Row's polish pass. A Row whose child changes size re-lays out while the frame is being
// drawn, and the basic render loop then draws a second, empty frame, so a reading that changes width every tick
// would cost two frames a tick.
Item {
    id: root

    property real spacing: 0

    readonly property var laidOut: Array.from(children).filter(child => child.visible && child.width > 0 && child.height > 0)
    readonly property var offsets: {
        let next = 0;
        return laidOut.map(child => {
            const offset = next;
            next += child.width + spacing;
            return offset;
        });
    }

    implicitWidth: laidOut.reduce((sum, child) => sum + child.width, 0) + spacing * Math.max(0, laidOut.length - 1)
    implicitHeight: laidOut.reduce((tallest, child) => Math.max(tallest, child.height), 0)

    function place() {
        laidOut.forEach((child, i) => {
            child.x = offsets[i];
        });
    }

    onOffsetsChanged: place()
    Component.onCompleted: place()
}
