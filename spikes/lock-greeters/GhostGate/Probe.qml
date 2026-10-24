import QtQuick
import "gate.mjs" as Gate

QtObject {
    function hello(where) {
        return Gate.greet(where);
    }
}
