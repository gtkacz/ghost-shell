import QtQuick

// A MouseArea for a control on the dashboard. The desktop's window takes clicks only inside its input region and
// passes every other click through to the desktop, so this area hands its rect to the view it sits in, which adds
// it to that region while the area is visible and enabled.
MouseArea {
    id: root

    // Whether the view takes clicks here, as visible and enabled include every ancestor's.
    readonly property bool claimsInput: visible && enabled
    // Logical pixels in the window.
    readonly property rect windowRect: tracking.rect

    cursorShape: Qt.PointingHandCursor

    onXChanged: tracking.place()
    onYChanged: tracking.place()
    onWidthChanged: tracking.place()
    onHeightChanged: tracking.place()
    onParentChanged: tracking.follow()
    Component.onCompleted: tracking.follow()
    Component.onDestruction: tracking.leave()

    // Mapping a point into the window is not a binding, so the rect is placed again whenever this area or any of its
    // ancestors moves, and the ancestors are followed again whenever one of them is reparented.
    QtObject {
        id: tracking

        property rect rect
        property var ancestors: []
        // The nearest ancestor that takes input rects: a layout view.
        property Item host: null

        function follow() {
            const chain = [];
            let found = null;
            for (let item = root.parent; item; item = item.parent) {
                chain.push(item);
                if (found === null && typeof item.claimInput === "function") {
                    found = item;
                }
            }
            ancestors = chain;
            if (found !== host) {
                leave();
                host = found;
                if (host) {
                    host.claimInput(root);
                }
            }
            place();
        }

        function place() {
            const mapped = root.mapToItem(null, 0, 0, root.width, root.height);
            rect = Qt.rect(mapped.x, mapped.y, mapped.width, mapped.height);
        }

        function leave() {
            if (host) {
                host.releaseInput(root);
            }
            host = null;
        }
    }

    // Connections rather than connect(): a view being destroyed unparents its items before it deletes them, and the
    // engine stops calling a Connections once its context is gone, where a connected function would still run.
    Instantiator {
        model: tracking.ancestors

        delegate: Connections {
            required property Item modelData

            target: modelData

            function onXChanged() {
                tracking.place();
            }

            function onYChanged() {
                tracking.place();
            }

            function onParentChanged() {
                tracking.follow();
            }
        }
    }
}
