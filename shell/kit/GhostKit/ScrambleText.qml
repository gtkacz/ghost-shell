import QtQuick
import "lib/scramble.mjs" as ScrambleLib
import "lib/firstwrite.mjs" as FirstWriteLib

// A Text that scrambles into place the first time it is written and is a plain Text afterwards. The
// first write belongs to the key, scoped by the host showing the widget, rather than to the item, so a
// view that is rebuilt does not scramble again but a widget shown anew does. Every step lands on a
// FrameClock tick, so it adds no frames of its own.
Text {
    id: root

    // The mockup's scramble(el, to, dur = 520).
    property real durationMs: 520
    // Names the write; a key that changes, as a track's does, scrambles again if FirstWrite has not seen it.
    // Bind it to the text's source, never to text, which the scramble overwrites each frame. Empty names
    // the write by the item's place under its host, so its first written value scrambles and later
    // values, such as a reading's, show directly.
    property string scrambleKey

    // Widgets do not know their host, so the scope is found up the tree: the nearest WidgetHost's, or
    // none outside one.
    function hostScope() {
        for (let item = parent; item; item = item.parent) {
            if (item.firstWriteScope !== undefined) {
                return item.firstWriteScope;
            }
        }
        return "";
    }

    // Stops below the host: a host destroys its previous widget only later, so the widget's own index
    // among the host's children differs between showings.
    function positionKey() {
        const steps = [];
        for (let item = root; item.parent && item.parent.firstWriteScope === undefined; item = item.parent) {
            const siblings = item.parent.children;
            for (let index = 0; index < siblings.length; index++) {
                if (siblings[index] === item) {
                    steps.unshift(index);
                    break;
                }
            }
        }
        return FirstWriteLib.positionKey(steps);
    }

    function considerKey() {
        if (!visible) {
            return;
        }
        const key = scrambleKey !== "" ? scrambleKey : FirstWriteLib.isWritten(text) ? positionKey() : "";
        if (key === "" || key === run.decidedKey) {
            return;
        }
        run.decidedKey = key;
        // Ends a scramble still running for the previous key, which hands the text back to its binding.
        run.stop();
        if (text !== "" && FirstWrite.claim(FirstWriteLib.scopedKey(hostScope(), key))) {
            run.begin(text);
        }
    }

    // Callers pass track titles and notification text, and a frame holds < and >, which AutoText would
    // render as markup.
    textFormat: Text.PlainText

    // Bindings that follow one datum, such as a title and its key, update in no fixed order, so the
    // decision waits until all of them have.
    onScrambleKeyChanged: Qt.callLater(considerKey)
    onTextChanged: {
        if (scrambleKey === "" && run.decidedKey === "") {
            Qt.callLater(considerKey);
        }
    }
    onVisibleChanged: considerKey()
    Component.onCompleted: considerKey()
    Component.onDestruction: run.stop()

    QtObject {
        id: run

        property bool active: false
        property string target
        property real startedMs: 0
        property string decidedKey

        readonly property real progress: active ? Math.min(1, (FrameClock.elapsedMs - startedMs) / root.durationMs) : 0

        function begin(finalText) {
            target = finalText;
            startedMs = FrameClock.elapsedMs;
            FrameClock.acquire();
            active = true;
        }

        function stop() {
            if (!active) {
                return;
            }
            active = false;
            FrameClock.release();
        }
    }

    // Takes the text over for the length of the scramble and then gives back whatever bound it, so the
    // caller's own binding is still live afterwards.
    Binding {
        target: root
        property: "text"
        value: ScrambleLib.scrambleFrame(run.target, run.progress, Math.random)
        when: run.active
    }

    Connections {
        target: FrameClock
        enabled: run.active

        // elapsedMs changes last in a tick, so this sees the time the frame was drawn for.
        function onElapsedMsChanged() {
            if (FrameClock.elapsedMs - run.startedMs >= root.durationMs) {
                run.stop();
            }
        }
    }
}
