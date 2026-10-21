import QtQuick
import org.kde.kwin

import "../code/main.js" as Desktop

// Declarative rather than a plain JavaScript script: KWin tells a JavaScript script nothing when it
// unloads it, and Component.onDestruction lets this one take back its last report.
Item {
    id: script

    DBusCall {
        id: reportCall

        service: "org.ghostshell.Ghostd"
        path: "/org/ghostshell/Desktop"
        dbusInterface: "org.ghostshell.Desktop1"
        method: "Report"
    }

    // KWin's scripting API has no show-desktop state, but KWin's own D-Bus interface has.
    DBusCall {
        id: showingDesktopQuery

        service: "org.kde.KWin"
        path: "/KWin"
        dbusInterface: "org.freedesktop.DBus.Properties"
        method: "Get"
        arguments: ["org.kde.KWin", "showingDesktop"]

        onFinished: returnValue => Desktop.setShowingDesktop(returnValue[0])
    }

    Component.onCompleted: Desktop.start({
        owner: script,
        workspace: Workspace,
        maximizeArea: Workspace.MaximizeArea,
        sendReport: (covered, active) => {
            reportCall.arguments = [covered, active];
            reportCall.call();
        },
        queryShowingDesktop: () => showingDesktopQuery.call(),
    })
    Component.onDestruction: Desktop.stop()
}
