import QtQuick
import QtTest
import GhostGate

// Not part of `make test-gate`: `make gate-shots` renders the screenshots the user compares with the mockup.
TestCase {
    id: test

    name: "Shots"
    when: windowShown
    visible: true
    width: 2226
    height: 1252

    readonly property string outDir: Qt.resolvedUrl("../../../docs/superpowers/notes/lock-a-shots/").toString().replace("file://", "")

    Component {
        id: screenComponent

        GateScreen {
            adapter: FakeAdapter {}
            primary: true
            width: test.width
            height: test.height
        }
    }

    function shoot(screen, name) {
        let saved = false;
        waitForRendering(screen);
        screen.grabToImage(function (result) {
            saved = result.saveToFile(test.outDir + name + ".png");
        });
        tryVerify(function () {
            return saved;
        }, 5000);
    }

    function test_lock_idle() {
        shoot(createTemporaryObject(screenComponent, test, {}), "lock-idle");
    }

    function test_lock_alert() {
        const screen = createTemporaryObject(screenComponent, test, {});
        for (const ch of "twelve chars") {
            keyClick(ch);
        }
        screen.adapter.attempts = 1;
        screen.adapter.failed();
        wait(1800);
        shoot(screen, "lock-alert");
    }

    function test_login_idle() {
        const screen = createTemporaryObject(screenComponent, test, {});
        screen.adapter.mode = "login";
        shoot(screen, "login-idle");
    }
}
