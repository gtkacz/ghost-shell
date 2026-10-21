import QtQuick

// The latest message on one topic, as Data.topic() hands it out. Each topic has its own object, so a
// message re-evaluates only the bindings that read its topic.
QtObject {
    readonly property var value: dataControl.value
    readonly property int seq: dataControl.seq
    readonly property double receivedMs: dataControl.receivedMs

    // Read-only above so that one widget cannot overwrite a topic every other widget reads; Data
    // writes it through dataControl, the way the host writes Theme.hostControl.
    readonly property QtObject dataControl: QtObject {
        property var value: null
        property int seq: 0
        property double receivedMs: 0
    }
}
