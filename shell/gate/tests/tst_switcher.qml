import QtQuick
import QtTest
import GhostGate
import "../GhostGate/lib/gate.mjs" as GateLib
import "../GhostGate/lib/switcher.mjs" as Switcher

TestCase {
    id: test

    readonly property int screenWidth: 2560

    name: "Switcher"
    when: windowShown
    visible: true
    width: 1920
    height: 1080

    Component {
        id: modelComponent

        ListModel {}
    }

    Component {
        id: panelComponent

        SwitcherPanel {}
    }

    SignalSpy {
        id: activatedSpy

        signalName: "activated"
    }

    // Every visible text in the item tree, so a test reads what a person would read.
    function texts(item, found) {
        const out = found || [];
        if (item.visible && item.text !== undefined && item.text !== "") {
            out.push(String(item.text));
        }
        for (let i = 0; i < item.children.length; i += 1) {
            texts(item.children[i], out);
        }
        return out;
    }

    function textItems(item, found) {
        const out = found || [];
        if (item.textFormat !== undefined) {
            out.push(item);
        }
        for (let i = 0; i < item.children.length; i += 1) {
            textItems(item.children[i], out);
        }
        return out;
    }

    function rows(count) {
        const model = createTemporaryObject(modelComponent, test);
        for (let i = 0; i < count; i += 1) {
            model.append({
                caption: "title" + i + " - app" + i,
                windowId: "w" + i,
                icon: "application-x-executable"
            });
        }
        return model;
    }

    function makePanel(model, currentIndex) {
        const panel = createTemporaryObject(panelComponent, test, {
            model: model,
            currentIndex: currentIndex
        });
        verify(panel);
        return panel;
    }

    function printed(panel) {
        tryCompare(panel, "printStep", Switcher.PRINT_STEPS);
    }

    function init() {
        failOnWarning(/.?/);
        Theme.fit(screenWidth);
    }

    function cleanup() {
        Theme.load(GateLib.parseSnapshot({}));
        Theme.u = 1;
        activatedSpy.target = null;
        activatedSpy.clear();
    }

    function test_rows_header_and_count() {
        const panel = makePanel(rows(3), 1);
        verify(texts(panel).includes("SWITCH WINDOW"));
        verify(texts(panel).includes("窓切替"));
        verify(texts(panel).includes("2 / 3"));
        verify(texts(panel).includes("01"));
        verify(texts(panel).includes("APP1"));
        verify(texts(panel).includes("title1"));
        compare(panel.currentWindowId, "w1");
    }

    function test_japanese_off_hides_the_tag() {
        Theme.load(GateLib.parseSnapshot({
            appearance: {
                japanese: "off"
            }
        }));
        const panel = makePanel(rows(1), 0);
        verify(!texts(panel).includes("窓切替"));
        verify(texts(panel).includes("SWITCH WINDOW"));
    }

    function test_title_markup_stays_literal() {
        const model = rows(0);
        model.append({
            caption: "<b>x</b>",
            windowId: "w0",
            icon: "application-x-executable"
        });
        const panel = makePanel(model, 0);
        verify(texts(panel).includes("<b>x</b>"));
        for (const item of textItems(panel)) {
            if (String(item.text).includes("<")) {
                compare(item.textFormat, Text.PlainText);
            }
        }
    }

    function test_text_is_plain_at_once() {
        const panel = makePanel(rows(2), 0);
        verify(panel.printStep < Switcher.PRINT_STEPS);
        verify(texts(panel).includes("SWITCH WINDOW"));
        verify(texts(panel).includes("title0"));
        verify(texts(panel).includes("title1"));
    }

    function test_size_does_not_change_with_the_list() {
        const few = makePanel(rows(3), 0);
        const many = makePanel(rows(15), 0);
        compare(few.width, many.width);
        compare(few.height, many.height);
    }

    function test_selection_stays_visible_past_ten_rows() {
        const panel = makePanel(rows(15), 14);
        const list = findChild(panel, "list");
        const selected = list.currentItem;
        verify(selected);
        verify(selected.y >= list.contentY);
        verify(selected.y + selected.height <= list.contentY + list.height);
        verify(findChild(panel, "more-above").visible);
        verify(!findChild(panel, "more-below").visible);
    }

    // The layout loads the theme once the panel is built, so on a session's first opening the rows grow under it.
    function test_selection_stays_visible_when_the_panel_is_fitted_late() {
        Theme.u = 1;
        const panel = makePanel(rows(15), 14);
        const list = findChild(panel, "list");
        Theme.fit(screenWidth);
        // The rows move only when the list next lays out, so before that their old positions would pass the check.
        waitForRendering(list);
        tryVerify(() => list.currentItem.y + list.currentItem.height <= list.contentY + list.height && list.currentItem.y >= list.contentY);
    }

    function test_no_chevron_when_nothing_is_hidden() {
        const panel = makePanel(rows(10), 9);
        verify(!findChild(panel, "more-above").visible);
        verify(!findChild(panel, "more-below").visible);
    }

    function test_a_row_removed_while_open_keeps_a_valid_selection() {
        const model = rows(3);
        const panel = makePanel(model, 2);
        model.remove(2);
        panel.currentIndex = 1;
        compare(panel.currentWindowId, "w1");
        verify(texts(panel).includes("2 / 2"));
    }

    // KWin's stacking order, which nameOf reads there, has no change signal: in a binding it logs a line per row per opening.
    function test_a_row_asks_for_its_name_once() {
        const model = rows(1);
        let asked = 0;
        const panel = createTemporaryObject(panelComponent, test, {
            model: model,
            currentIndex: 0,
            nameOf: () => {
                asked += 1;
                return "NAME";
            }
        });
        verify(panel);
        compare(asked, 1);
        verify(texts(panel).includes("NAME"));
        model.setProperty(0, "caption", "new title");
        compare(asked, 1);
        verify(texts(panel).includes("new title"));
    }

    function test_one_or_no_windows() {
        const one = makePanel(rows(1), 0);
        verify(texts(one).includes("1 / 1"));
        compare(one.currentWindowId, "w0");
        const none = makePanel(rows(0), 0);
        verify(texts(none).includes("0 / 0"));
        compare(none.currentWindowId, undefined);
    }

    function test_click_activates_its_row() {
        const panel = makePanel(rows(3), 0);
        printed(panel);
        activatedSpy.target = panel;
        mouseClick(findChild(panel, "list").itemAtIndex(2));
        compare(activatedSpy.count, 1);
        compare(activatedSpy.signalArguments[0][0], 2);
    }

    function test_print_runs_once_to_full() {
        const panel = makePanel(rows(1), 0);
        verify(panel.printStep < Switcher.PRINT_STEPS);
        tryCompare(panel, "printStep", Switcher.PRINT_STEPS, Switcher.PRINT_MS * 3);
        verify(!findChild(panel, "printer").running);
    }

    function test_icon_style_follows_the_setting() {
        const mono = makePanel(rows(1), 0);
        verify(findChild(mono, "row-icon").mono);
        Theme.load(GateLib.parseSnapshot({
            appearance: {
                icons: "colour"
            }
        }));
        const colour = makePanel(rows(1), 0);
        verify(!findChild(colour, "row-icon").mono);
    }

    function test_fits_a_smaller_screen() {
        Theme.fit(1920);
        const panel = makePanel(rows(1), 0);
        verify(panel.width < 1920);
    }
}
