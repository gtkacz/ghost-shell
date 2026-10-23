// Run through org.kde.PlasmaShell.evaluateScript. Removes every panel and puts a System Tray and a Digital Clock
// on screen 0's desktop, at its top right, carrying over the panel's tray and clock config. Prints one line per step.
const TRAY = "org.kde.plasma.systemtray";
const CLOCK = "org.kde.plasma.digitalclock";
// The bar's strip at scale 1.15, in logical px: both widgets sit under it.
const STRIP_HEIGHT = 52;
const TRAY_WIDTH = 480;
const CLOCK_WIDTH = 240;

// readConfig hands a list back as one comma-joined string, and writeConfig would store that string's characters.
const LIST_KEYS = ["extraItems", "hiddenItems", "shownItems", "knownItems", "disabledStatusNotifiers",
    "enabledCalendarPlugins", "selectedTimeZones"];
const lines = [];

// A walk over configGroups that changes currentConfigGroup visited only one group, so each group is named.
function copyConfig(from, to, group) {
    from.currentConfigGroup = [group];
    to.currentConfigGroup = [group];
    const keys = from.configKeys;
    for (let i = 0; i < keys.length; i++) {
        const value = from.readConfig(keys[i]);
        const isList = LIST_KEYS.indexOf(keys[i]) >= 0;
        to.writeConfig(keys[i], isList ? (value === "" ? [] : value.split(",")) : value);
    }
    to.reloadConfig();
    return group + ": " + keys.join(",");
}

const desktop = desktops().filter(function (d) { return d.screen === 0; })[0];
const area = screenGeometry(0);
lines.push("desktop " + desktop.id + " " + desktop.type + " screen " + area.width + "x" + area.height);

let oldTray = null;
let oldClock = null;
panels().forEach(function (panel) {
    oldTray = oldTray || panel.widgets(TRAY)[0] || null;
    oldClock = oldClock || panel.widgets(CLOCK)[0] || null;
});

const tray = desktop.addWidget(TRAY, area.width - CLOCK_WIDTH - TRAY_WIDTH, 0, TRAY_WIDTH, STRIP_HEIGHT);
const clock = desktop.addWidget(CLOCK, area.width - CLOCK_WIDTH, 0, CLOCK_WIDTH, STRIP_HEIGHT);
lines.push("added tray " + tray.id + " clock " + clock.id);
if (oldTray) {
    lines.push("tray config " + copyConfig(oldTray, tray, "General"));
}
if (oldClock) {
    lines.push("clock config " + copyConfig(oldClock, clock, "Appearance"));
}

panels().forEach(function (panel) {
    lines.push("removing panel " + panel.id + " at " + panel.location);
    panel.remove();
});

[tray, clock].forEach(function (widget) {
    const g = widget.geometry;
    lines.push("placed " + widget.type + " " + widget.id + " at " + g.x + "," + g.y + " " + g.width + "x" + g.height);
});
print(lines.join("\n"));
