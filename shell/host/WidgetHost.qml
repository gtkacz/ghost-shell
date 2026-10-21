import QtQuick

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

    function load() {
        unload();
        if (!slot) {
            return;
        }
        if (slot.error) {
            failure = slot.error;
            return;
        }
        const component = Qt.createComponent(slot.source);
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
            fail(component.errorString().split("\n")[0]);
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

    Loader {
        anchors.fill: parent
        active: root.failure !== ""

        sourceComponent: ErrorCard {
            widgetId: root.widgetId
            message: root.failure
        }
    }
}
