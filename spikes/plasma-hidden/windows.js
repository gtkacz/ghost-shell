// A KWin script that logs every window KWin stacks, with its logical frame geometry, for run.sh's `windows`.
const tag = "gs-windows";
console.info(tag, "begin");
workspace.stackingOrder.forEach(function (w) {
    const g = w.frameGeometry;
    console.info(tag, JSON.stringify({
        resourceClass: w.resourceClass, caption: w.caption, x: g.x, y: g.y, width: g.width, height: g.height,
        popup: w.popupWindow, dock: w.dock, desktopWindow: w.desktopWindow, normal: w.normalWindow,
        dialog: w.dialog, onScreenDisplay: w.onScreenDisplay, notification: w.notification,
    }));
});
console.info(tag, "end");
