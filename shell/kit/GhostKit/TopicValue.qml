import QtQuick

// The latest message on one topic, as Data.topic() hands it out. Each topic has its own object, so a
// message re-evaluates only the bindings that read its topic.
//
// value, seq and receivedMs change only when a new message arrives. The daemon's replay of a message
// already received on this connection changes nothing, so receivedMs is when the data was new; the
// first message after a reconnect is always taken, even with the seq the topic already shows.
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
