import QtQuick

// Deliberately invalid: the object below is never closed, so the component fails to compile.
Item {
    property var settings
