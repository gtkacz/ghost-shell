import QtQuick
import org.kde.kwin

import "../code/main.js" as Desktop

// Declarative rather than a plain JavaScript script: KWin tells a JavaScript script nothing when it
// unloads it, and Component.onDestruction lets this one take back its last report.
Item {
    id: script

    readonly property int retryMs: 1000

    DBusCall {
        id: reportCall

        service: "org.ghostshell.Ghostd"
        path: "/org/ghostshell/Desktop"
        dbusInterface: "org.ghostshell.Desktop1"
        method: "Report"
    }

    DBusCall {
        id: windowsCall

        service: "org.ghostshell.Ghostd"
        path: "/org/ghostshell/Windows"
        dbusInterface: "org.ghostshell.Windows1"
        method: "Report"

        onFailed: Desktop.log("report failed")
    }

    // ghostd holds each Next call open until it has a command or its timeout ends, so exactly one call is
    // in flight while the bridge is up.
    DBusCall {
        id: nextCall

        service: "org.ghostshell.Ghostd"
        path: "/org/ghostshell/Windows"
        dbusInterface: "org.ghostshell.WindowCommands1"
        method: "Next"

        // The next poll goes out first: a command that throws would otherwise end the loop for good.
        onFinished: returnValue => {
            Desktop.log("next finished " + JSON.stringify(returnValue));
            nextCall.call();
            Desktop.handleCommand(returnValue[0]);
        }
        onFailed: {
            Desktop.log("next failed");
            retryTimer.start();
        }
    }

    Timer {
        id: retryTimer

        interval: script.retryMs
        // A restarted ghostd has no list until a window changes, so each retry offers it again.
        onTriggered: {
            Desktop.resendWindows();
            nextCall.call();
        }
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

    Component.onCompleted: {
        Desktop.start({
            owner: script,
            workspace: Workspace,
            maximizeArea: Workspace.MaximizeArea,
            sendReport: (covered, active) => {
                reportCall.arguments = [covered, active];
                reportCall.call();
            },
            sendWindows: json => {
                windowsCall.arguments = [json];
                windowsCall.call();
            },
            queryShowingDesktop: () => showingDesktopQuery.call(),
        });
        nextCall.call();
    }
    Component.onDestruction: Desktop.stop()
}
