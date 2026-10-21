import QtQuick
import GhostKit

// Shows one layout slot: its widget, created from the slot's source with the slot's settings, or an
// error card naming the widget and the reason it can't be shown.
Item {
    id: root

    // A slot as computeRects places it: widget, source, settings and error.
    property var slot: null

    readonly property string widgetId: slot ? slot.widget : ""
    property string failure: ""
    // The widget object, while one is shown.
    property var instance: null
    // A component still loading, which only a remote or very large source does.
    property var pending: null
    property bool completed: false
    // The URL the current component was requested from.
    property string requested: ""

    function load() {
        unload();
        if (!slot) {
            return;
        }
        if (slot.error) {
            failure = slot.error;
            return;
        }
        request(slot.source);
    }

    // The engine keeps a failed compile for the URL it was loaded from, so a fixed Widget.qml would
    // keep failing from its own URL until the shell restarts; the query gives the engine a URL it has
    // not compiled yet. Only Widget.qml is reread that way: QML files it imports may stay cached.
    // A time rather than a per-host count, so a host never reuses a URL another host already failed on.
    function freshSource() {
        return slot.source + "?r=" + Date.now();
    }

    function request(url) {
        requested = url;
        const component = Qt.createComponent(url);
        if (component.status === Component.Loading) {
            pending = component;
            // A method rather than a closure, so the connection dies with this host.
            component.statusChanged.connect(settlePending);
            return;
        }
        settle(component);
    }

    function settlePending() {
        if (pending.status === Component.Loading) {
            return;
        }
        const component = pending;
        stopWaiting();
        settle(component);
    }

    function settle(component) {
        if (component.status === Component.Error) {
            // A failure from the slot's own URL may be the cached one from before a fix.
            if (requested === slot.source) {
                request(freshSource());
                return;
            }
            fail(component.errorString().split("\n")[0].replace(requested, slot.source));
            return;
        }
        instance = component.createObject(root, {
            settings: slot.settings
        });
        if (instance === null) {
            fail("its root object could not be created");
        }
    }

    function fail(reason) {
        failure = "widget \"" + widgetId + "\" failed to load: " + reason;
        console.warn(failure);
    }

    function stopWaiting() {
        if (pending) {
            pending.statusChanged.disconnect(settlePending);
            pending = null;
        }
    }

    function unload() {
        stopWaiting();
        failure = "";
        if (instance) {
            instance.destroy();
            instance = null;
        }
    }

    // A slot set while the host is being created is loaded once, on completion.
    onSlotChanged: {
        if (completed) {
            load();
        }
    }
    Component.onCompleted: {
        completed = true;
        load();
    }
    Component.onDestruction: stopWaiting()

    // The daemon re-sends the layout whenever a widget folder changes, which is when a broken widget
    // may have been fixed. A layout equal to the last one keeps this host and its slot, so nothing
    // else would load the widget again.
    Connections {
        target: Data

        function onLayoutChanged() {
            if (root.failure !== "" && root.slot && !root.slot.error) {
                root.load();
            }
        }
    }

    Loader {
        anchors.fill: parent
        active: root.failure !== ""

        sourceComponent: ErrorCard {
            widgetId: root.widgetId
            message: root.failure
        }
    }
}
