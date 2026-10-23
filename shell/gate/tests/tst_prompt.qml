import QtQuick
import QtTest
import GhostGate
import "../GhostGate/lib/gate.mjs" as GateLib

TestCase {
    id: test

    name: "Prompt"
    when: windowShown
    visible: true
    width: 1280
    height: 720

    Component {
        id: screenComponent

        GateScreen {
            adapter: FakeAdapter {}
            primary: true
            width: test.width
            height: test.height
        }
    }

    SignalSpy {
        id: doneSpy

        signalName: "done"
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

    function find(item, objectName) {
        return findChild(item, objectName);
    }

    function makeScreen(properties) {
        const screen = createTemporaryObject(screenComponent, test, {});
        verify(screen);
        waitForRendering(screen);
        for (const key in properties || {}) {
            screen.adapter[key] = properties[key];
        }
        return screen;
    }

    function type(text) {
        for (const ch of text) {
            keyClick(ch);
        }
    }

    function cleanup() {
        Theme.load(GateLib.parseSnapshot({}));
        doneSpy.target = null;
        doneSpy.clear();
    }

    function test_cells_light_per_key() {
        const screen = makeScreen();
        const password = find(screen, "password");
        type("abc");
        compare(password.lit, 3);
        compare(password.cursor, 3);
    }

    function test_long_password_caps() {
        const password = find(makeScreen(), "password");
        type("a".repeat(40));
        compare(password.lit, 28);
        compare(password.cursor, 27);
    }

    function test_escape_clears() {
        const password = find(makeScreen(), "password");
        type("abc");
        keyClick(Qt.Key_Escape);
        compare(password.lit, 0);
    }

    function test_wrong_password_shows_alert() {
        const screen = makeScreen();
        const alert = find(screen, "alert");
        const password = find(screen, "password");
        verify(!alert.visible);
        type("nope");
        keyClick(Qt.Key_Return);
        verify(alert.visible);
        compare(alert.text, "ACCESS DENIED · ATTEMPT 1");
        compare(password.lit, 0);
        type("x");
        verify(!alert.visible);
    }

    function test_busy_ignores_keys_and_enter() {
        const screen = makeScreen({ holdBusy: true });
        const password = find(screen, "password");
        type("abc");
        keyClick(Qt.Key_Return);
        compare(screen.adapter.submitCount, 1);
        compare(password.lit, 3);
        verify(!password.cursorShown);
        type("xyz");
        keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);
        compare(password.lit, 3);
        compare(screen.adapter.submitCount, 1);
    }

    function test_busy_ending_without_a_verdict_clears_the_field() {
        const screen = makeScreen({ holdBusy: true });
        const password = find(screen, "password");
        type("abc");
        keyClick(Qt.Key_Return);
        compare(password.lit, 3);
        screen.adapter.busy = false;
        compare(password.lit, 0);
    }

    function test_full_row_keeps_cursor_off_the_last_cell() {
        const password = find(makeScreen(), "password");
        type("abc");
        verify(password.cursorShown);
        type("a".repeat(40));
        compare(password.lit, 28);
        verify(!password.cursorShown);
    }

    function test_alert_leaves_everything_above_it_in_place() {
        const screen = makeScreen();
        const password = find(screen, "password");
        const user = find(screen, "userName");
        const before = [password.mapToItem(null, 0, 0).y, user.mapToItem(null, 0, 0).y];
        type("nope");
        keyClick(Qt.Key_Return);
        verify(find(screen, "alert").visible);
        waitForRendering(screen);
        compare(password.mapToItem(null, 0, 0).y, before[0]);
        compare(user.mapToItem(null, 0, 0).y, before[1]);
    }

    function test_tag_after_the_name_is_host_when_locked_and_session_at_login() {
        Theme.load(GateLib.parseSnapshot({ gate: { host: "ghost-shell" } }));
        const screen = makeScreen();
        verify(texts(screen).includes("ghost-shell"));
        screen.adapter.mode = "login";
        verify(!texts(screen).includes("ghost-shell"));
        verify(texts(screen).includes("Plasma · Wayland"));
    }

    function test_secondary_screen_never_reports_done() {
        const screen = makeScreen();
        const other = createTemporaryObject(screenComponent, test, { primary: false, adapter: screen.adapter });
        doneSpy.target = other;
        screen.adapter.succeeded();
        wait(1200);
        compare(doneSpy.count, 0);
    }

    function test_empty_enter_does_nothing() {
        const screen = makeScreen();
        keyClick(Qt.Key_Return);
        compare(screen.adapter.submitCount, 0);
    }

    function test_pam_message_replaces_alert() {
        const screen = makeScreen();
        screen.adapter.message = "Account locked";
        const alert = find(screen, "alert");
        verify(alert.visible);
        compare(alert.text, "ACCOUNT LOCKED");
    }

    function test_caps_chip() {
        const screen = makeScreen();
        verify(!texts(screen).includes("CAPS LOCK"));
        screen.adapter.capsLock = true;
        verify(texts(screen).includes("CAPS LOCK"));
    }

    function test_click_refocuses() {
        const screen = makeScreen();
        test.forceActiveFocus();
        mouseClick(screen, 10, 10);
        type("ab");
        compare(find(screen, "password").lit, 2);
    }

    function test_user_chip_only_with_many_users() {
        const screen = makeScreen();
        const user = find(screen, "userName");
        verify(!user.chipped);
        screen.adapter.users = ["a", "b"];
        verify(user.chipped);
        compare(screen.adapter.userName, "a");
        mouseClick(user);
        compare(screen.adapter.userName, "b");
    }

    function test_layout_chip_cycles() {
        const screen = makeScreen();
        verify(texts(screen).includes("us"));
        mouseClick(find(screen, "layoutChip"));
        compare(screen.adapter.layout, "br");
        verify(texts(screen).includes("br"));
        mouseClick(find(screen, "layoutChip"));
        compare(screen.adapter.layout, "us");
    }

    function test_lock_vs_login_actions() {
        const screen = makeScreen();
        compare(find(screen, "actions").labels, ["SWITCH USER", "SLEEP"]);
        verify(!find(screen, "sessionChip").visible);
        screen.adapter.mode = "login";
        compare(find(screen, "actions").labels, ["SLEEP", "RESTART", "SHUT DOWN"]);
        verify(find(screen, "sessionChip").visible);
    }

    function test_japanese_off_hides_tag() {
        const screen = makeScreen();
        verify(texts(screen).includes("認証"));
        Theme.load(GateLib.parseSnapshot({ appearance: { japanese: "off" } }));
        verify(!texts(screen).includes("認証"));
    }

    function test_success_lock_plays_pass_then_done() {
        const screen = makeScreen();
        doneSpy.target = screen;
        type("ghost");
        keyClick(Qt.Key_Return);
        compare(doneSpy.count, 0);
        tryCompare(doneSpy, "count", 1, 2000);
    }

    function test_success_login_done_immediately() {
        const screen = makeScreen({ mode: "login" });
        doneSpy.target = screen;
        type("ghost");
        keyClick(Qt.Key_Return);
        tryCompare(doneSpy, "count", 1, 100);
    }
}
