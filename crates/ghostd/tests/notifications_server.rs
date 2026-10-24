mod common;

use std::collections::HashMap;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixDatagram};

use common::bus::{BUS_WAIT, PrivateBus, next};
use common::notify_client::{Hints, NOTIFICATIONS, NOTIFICATIONS_PATH, NotifyClient};
use ghost_proto::{NotificationAction, NotificationLevel, Urgency};
use ghostd::feeds::notifications::history::{Excess, Incoming};
use ghostd::feeds::notifications::images::ImageData;
use ghostd::feeds::notifications::server::{
    self, CLOSED_BY_CALL, Claim, DISMISSED, EXPIRED, Request,
};
use ghostd::sdnotify;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;
use zbus::Connection;
use zbus::Variant;
use zbus::fdo::{self, RequestNameFlags};
use zbus::message::Message;
use zbus::proxy::CacheProperties;
use zbus::{Array, Structure};

const CRITICAL: u8 = 2;
const EXCESS_ERROR: &str = "org.freedesktop.Notifications.Error.ExcessNotificationGeneration";

struct Served {
    bus: PrivateBus,
    daemon: Connection,
    inbox: mpsc::Receiver<Request>,
    inhibited: watch::Receiver<bool>,
}

async fn serving() -> Served {
    let served = serving_unclaimed().await;
    assert_eq!(server::claim(&served.daemon).await.unwrap(), Claim::Owned);
    served
}

async fn serving_unclaimed() -> Served {
    let bus = PrivateBus::start();
    let daemon = bus.connect().await;
    let (sender, inbox) = mpsc::channel(16);
    let helper = bus.connect().await;
    let inhibited = server::serve(&daemon, &helper, sender).await.unwrap();
    Served {
        bus,
        daemon,
        inbox,
        inhibited,
    }
}

async fn received(inbox: &mut mpsc::Receiver<Request>) -> Request {
    timeout(BUS_WAIT, inbox.recv())
        .await
        .expect("no request in time")
        .unwrap()
}

/// Answers the next `Notify` with `answer`, returning what it carried.
async fn answer_notify(
    inbox: &mut mpsc::Receiver<Request>,
    answer: Result<u32, Excess>,
) -> (Incoming, Option<ImageData>) {
    let Request::Notify {
        incoming,
        image_data,
        reply,
    } = received(inbox).await
    else {
        panic!("expected a Notify");
    };
    reply.send(answer).unwrap();
    (incoming, image_data)
}

async fn notify_and_capture(
    served: &mut Served,
    client: &NotifyClient,
    app_name: &str,
    body: &str,
    actions: &[&str],
    hints: &Hints<'_>,
) -> (u32, Incoming, Option<ImageData>) {
    let (id, captured) = tokio::join!(
        client.notify(app_name, "summary", body, actions, hints),
        answer_notify(&mut served.inbox, Ok(7)),
    );
    (id.unwrap(), captured.0, captured.1)
}

fn picture() -> (ImageData, Variant<'static>) {
    let pixels = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
    let image = ImageData {
        width: 2,
        height: 1,
        rowstride: 8,
        has_alpha: true,
        bits_per_sample: 8,
        channels: 4,
        data: pixels.clone(),
    };
    let value = Structure::builder()
        .add_field(2i32)
        .add_field(1i32)
        .add_field(8i32)
        .add_field(true)
        .add_field(8i32)
        .add_field(4i32)
        .add_field(Array::from(pixels))
        .build()
        .unwrap();
    (image, Variant::Structure(value))
}

#[tokio::test]
async fn notify_reaches_the_inbox_with_parsed_hints() {
    let mut served = serving().await;
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;
    let (image, image_value) = picture();
    let hints: Hints = HashMap::from([
        ("urgency", Variant::from(CRITICAL)),
        ("desktop-entry", Variant::from("org.mozilla.firefox")),
        ("resident", Variant::from(true)),
        ("image-data", image_value),
        ("image-path", Variant::from("/tmp/ignored.png")),
    ]);

    let (id, incoming, image_data) = notify_and_capture(
        &mut served,
        &client,
        "Firefox",
        "<b>hi</b>",
        &["default", "Open", "mute", "Mute"],
        &hints,
    )
    .await;

    assert_eq!(id, 7);
    assert_eq!(incoming.app, "Firefox");
    assert_eq!(incoming.desktop_entry, "org.mozilla.firefox");
    assert_eq!(incoming.urgency, Urgency::Critical);
    assert_eq!(incoming.level, NotificationLevel::Crit);
    assert!(incoming.resident);
    assert!(!incoming.transient);
    assert_eq!(incoming.summary, "summary");
    assert_eq!(incoming.body, "<b>hi</b>");
    assert_eq!(incoming.body_text, "hi");
    assert_eq!(incoming.pid, std::process::id());
    assert_eq!(incoming.replaces_id, 0);
    assert_eq!(image_data, Some(image));
    assert_eq!(
        incoming.image, "/tmp/ignored.png",
        "the feed overwrites it once the picture is written"
    );
}

#[tokio::test]
async fn warn_level_default_action_and_unsafe_body_are_parsed() {
    let mut served = serving().await;
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;
    let hints: Hints = HashMap::from([
        ("x-ghost-shell-level", Variant::from("warn")),
        ("desktop-entry", Variant::from("kdeconnect")),
        ("transient", Variant::from(true)),
        ("image_path", Variant::from("file:///a.png")),
    ]);

    let (_, incoming, image_data) = notify_and_capture(
        &mut served,
        &client,
        "",
        "<u>b</u><script>x</script>",
        &["default", "Open", "reply", "Reply", "dangling"],
        &hints,
    )
    .await;

    assert_eq!(incoming.app, "kdeconnect");
    assert_eq!(incoming.level, NotificationLevel::Warn);
    assert_eq!(incoming.urgency, Urgency::Normal);
    assert!(incoming.transient);
    assert!(incoming.has_default);
    assert_eq!(
        incoming.actions,
        [NotificationAction {
            key: "reply".into(),
            label: "Reply".into()
        }]
    );
    assert_eq!(incoming.body, "<u>b</u>x");
    assert_eq!(incoming.body_text, "bx");
    assert_eq!(incoming.image, "file:///a.png");
    assert_eq!(image_data, None);
}

#[tokio::test]
async fn wrong_typed_hints_are_ignored() {
    let mut served = serving().await;
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;
    let hints: Hints = HashMap::from([
        ("urgency", Variant::from("critical")),
        ("desktop-entry", Variant::from(5i32)),
        ("resident", Variant::from("yes")),
        ("image-data", Variant::from("not a struct")),
    ]);

    let (_, incoming, image_data) =
        notify_and_capture(&mut served, &client, "App", "", &[], &hints).await;

    assert_eq!(incoming.urgency, Urgency::Normal);
    assert_eq!(incoming.level, NotificationLevel::None);
    assert_eq!(incoming.desktop_entry, "");
    assert!(!incoming.resident);
    assert_eq!(image_data, None);
}

#[tokio::test]
async fn excess_is_refused_under_the_standard_error_name() {
    let mut served = serving().await;
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;

    let no_hints = HashMap::new();
    let (result, _) = tokio::join!(
        client.notify("App", "s", "", &[], &no_hints),
        answer_notify(&mut served.inbox, Err(Excess)),
    );

    let Err(zbus::Error::MethodError(name, ..)) = result else {
        panic!("expected a method error, got {result:?}");
    };
    assert_eq!(name.as_str(), EXCESS_ERROR);
}

#[tokio::test]
async fn capabilities_and_server_information_are_exact() {
    let served = serving().await;
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;

    let capabilities: Vec<String> = client.proxy.call("GetCapabilities", &()).await.unwrap();
    let information: (String, String, String, String) = client
        .proxy
        .call("GetServerInformation", &())
        .await
        .unwrap();

    assert_eq!(
        capabilities,
        [
            "body",
            "body-markup",
            "body-hyperlinks",
            "actions",
            "persistence",
            "icon-static",
            "inhibitions"
        ]
    );
    assert_eq!(
        information,
        (
            "ghost-shell".to_owned(),
            "ghost-shell".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
            "1.2".to_owned()
        )
    );
}

#[tokio::test]
async fn close_notification_queues_a_close() {
    let mut served = serving().await;
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;

    let () = client
        .proxy
        .call("CloseNotification", &(5u32,))
        .await
        .unwrap();

    assert!(matches!(
        received(&mut served.inbox).await,
        Request::Close { id: 5 }
    ));
}

#[tokio::test]
async fn signals_reach_the_client() {
    let served = serving().await;
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;
    let mut closed = client
        .proxy
        .receive_signal("NotificationClosed")
        .await
        .unwrap();
    let mut invoked = client.proxy.receive_signal("ActionInvoked").await.unwrap();

    server::emit_closed(&served.daemon, 9, DISMISSED)
        .await
        .unwrap();
    server::emit_action_invoked(&served.daemon, 9, "reply")
        .await
        .unwrap();

    let message = next(&mut closed).await.unwrap();
    assert_eq!(
        message.body().deserialize::<(u32, u32)>().unwrap(),
        (9, DISMISSED)
    );
    let message = next(&mut invoked).await.unwrap();
    assert_eq!(
        message.body().deserialize::<(u32, String)>().unwrap(),
        (9, "reply".to_owned())
    );
    assert_eq!((EXPIRED, DISMISSED, CLOSED_BY_CALL), (1, 2, 3));
}

async fn property_changes(connection: &Connection) -> fdo::PropertiesChangedStream {
    fdo::PropertiesProxy::builder(connection)
        .destination(NOTIFICATIONS)
        .path(NOTIFICATIONS_PATH)
        .build()
        .await
        .unwrap()
        .receive_properties_changed()
        .await
        .unwrap()
}

async fn inhibit(client: &NotifyClient) -> u32 {
    client
        .proxy
        .call(
            "Inhibit",
            &("firefox", "presenting", HashMap::<&str, Variant>::new()),
        )
        .await
        .unwrap()
}

async fn becomes(watch: &mut watch::Receiver<bool>, value: bool) {
    timeout(BUS_WAIT, watch.wait_for(|&current| current == value))
        .await
        .expect("inhibition did not settle in time")
        .unwrap();
}

#[tokio::test]
async fn inhibit_sets_the_property_and_uninhibit_clears_it() {
    let served = serving().await;
    let mut inhibited = served.inhibited.clone();
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;
    let mut changes = property_changes(&app).await;
    assert!(!client.inhibited().await);

    let cookie = inhibit(&client).await;

    assert_ne!(cookie, 0);
    assert!(client.inhibited().await);
    assert!(*inhibited.borrow());
    let change = next(&mut changes).await.unwrap();
    assert_eq!(
        change
            .args()
            .unwrap()
            .changed_properties()
            .get("Inhibited")
            .map(|v| bool::try_from(v).unwrap()),
        Some(true)
    );

    let () = client.proxy.call("UnInhibit", &(cookie,)).await.unwrap();
    assert!(!client.inhibited().await);
    becomes(&mut inhibited, false).await;
}

#[tokio::test]
async fn uninhibit_of_an_unknown_cookie_is_a_no_op() {
    let served = serving().await;
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;
    inhibit(&client).await;

    let () = client.proxy.call("UnInhibit", &(99u32,)).await.unwrap();

    assert!(client.inhibited().await);
    assert!(*served.inhibited.borrow());
}

#[tokio::test]
async fn an_inhibitor_that_disconnects_is_released() {
    let served = serving().await;
    let mut inhibited = served.inhibited.clone();
    let app = served.bus.connect().await;
    let client = NotifyClient::new(&app).await;
    inhibit(&client).await;
    let watcher = served.bus.connect().await;
    // A connection shuts down gracefully only once every clone of it is gone.
    drop(client);
    let observer = NotifyClient::new(&watcher).await;
    assert!(observer.inhibited().await);

    timeout(BUS_WAIT, app.graceful_shutdown())
        .await
        .expect("shutdown hung");

    becomes(&mut inhibited, false).await;
    assert!(!observer.inhibited().await);
}

#[tokio::test]
async fn inhibitor_that_leaves_immediately_does_not_stick() {
    let served = serving().await;
    let mut inhibited = served.inhibited.clone();
    let observer_connection = served.bus.connect().await;
    let observer = NotifyClient::new(&observer_connection).await;
    // Many rounds, since the bug was a race between the call and the departure.
    for _ in 0..20 {
        let app = served.bus.connect().await;
        let call = Message::method_call(NOTIFICATIONS_PATH, "Inhibit")
            .destination(NOTIFICATIONS)
            .interface(NOTIFICATIONS)
            .build(&("firefox", "reason", HashMap::<&str, Variant>::new()))
            .unwrap();
        app.send(&call).await.unwrap();
        timeout(BUS_WAIT, app.graceful_shutdown())
            .await
            .expect("shutdown hung");
    }

    becomes(&mut inhibited, false).await;
    assert!(!observer.inhibited().await);
}

#[tokio::test]
async fn pipelined_replacements_arrive_in_send_order() {
    const CALLS: u32 = 20;
    let served = serving().await;
    let app = served.bus.connect().await;
    // Answers each call as the feed would, from its own task.
    let mut inbox = served.inbox;
    let feed = tokio::spawn(async move {
        let mut order = Vec::new();
        while order.len() < CALLS as usize {
            let Request::Notify {
                incoming, reply, ..
            } = received(&mut inbox).await
            else {
                panic!("expected a Notify");
            };
            order.push(incoming.summary);
            reply.send(Ok(5)).unwrap();
        }
        order
    });

    for number in 0..CALLS {
        let call = Message::method_call(NOTIFICATIONS_PATH, "Notify")
            .destination(NOTIFICATIONS)
            .interface(NOTIFICATIONS)
            .build(&(
                "App",
                5u32,
                "",
                number.to_string(),
                "",
                Vec::<&str>::new(),
                HashMap::<&str, Variant>::new(),
                -1i32,
            ))
            .unwrap();
        app.send(&call).await.unwrap();
    }

    let order = timeout(BUS_WAIT, feed).await.unwrap().unwrap();
    let expected: Vec<String> = (0..CALLS).map(|number| number.to_string()).collect();
    assert_eq!(order, expected);
}

fn holder_flags(allow_replacement: bool) -> RequestNameFlags {
    if allow_replacement {
        RequestNameFlags::DO_NOT_QUEUE | RequestNameFlags::ALLOW_REPLACEMENT
    } else {
        RequestNameFlags::DO_NOT_QUEUE
    }
}

async fn holder(bus: &PrivateBus, allow_replacement: bool) -> Connection {
    let holder = bus.connect().await;
    holder
        .request_name_with_flags(NOTIFICATIONS, holder_flags(allow_replacement))
        .await
        .unwrap();
    holder
}

#[tokio::test]
async fn claim_takes_the_name_from_a_holder_that_allows_replacement() {
    let served = serving_unclaimed().await;
    let plasma = holder(&served.bus, true).await;

    let claim = server::claim(&served.daemon).await.unwrap();

    assert_eq!(claim, Claim::Owned);
    let owner = fdo::DBusProxy::new(&plasma)
        .await
        .unwrap()
        .get_name_owner(NOTIFICATIONS.try_into().unwrap())
        .await
        .unwrap();
    assert_eq!(
        owner.as_str(),
        served.daemon.unique_name().unwrap().as_str()
    );
}

#[tokio::test]
async fn queues_behind_a_holder_that_forbids_replacement_and_inherits() {
    let served = serving_unclaimed().await;
    let plasma = holder(&served.bus, false).await;
    let watcher = served.bus.connect().await;
    let mut owners = fdo::DBusProxy::builder(&watcher)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .unwrap()
        .receive_name_owner_changed_with_args(&[(0, NOTIFICATIONS)])
        .await
        .unwrap();

    let claim = server::claim(&served.daemon).await.unwrap();

    assert_eq!(
        claim,
        Claim::Queued {
            owner: plasma.unique_name().unwrap().to_string()
        }
    );
    plasma.graceful_shutdown().await;
    let ours = served.daemon.unique_name().unwrap().to_string();
    loop {
        let change = timeout(BUS_WAIT, next(&mut owners)).await.unwrap().unwrap();
        let args = change.args().unwrap();
        if args.new_owner().as_ref().map(|owner| owner.as_str()) == Some(ours.as_str()) {
            break;
        }
    }
}

#[tokio::test]
async fn ready_is_sent_while_queued() {
    let served = serving_unclaimed().await;
    let _plasma = holder(&served.bus, false).await;
    assert!(matches!(
        server::claim(&served.daemon).await.unwrap(),
        Claim::Queued { .. }
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notify.sock");
    let by_path = UnixDatagram::bind(&path).unwrap();
    let abstract_name = format!("ghost-shell-test-{}", std::process::id());
    let by_name =
        UnixDatagram::bind_addr(&SocketAddr::from_abstract_name(&abstract_name).unwrap()).unwrap();

    sdnotify::send_ready(path.to_str().unwrap()).unwrap();
    sdnotify::send_ready(&format!("@{abstract_name}")).unwrap();

    for socket in [by_path, by_name] {
        let mut buffer = [0u8; 16];
        let length = socket.recv(&mut buffer).unwrap();
        assert_eq!(&buffer[..length], b"READY=1");
    }
}
