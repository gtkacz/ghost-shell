//! A fake UDisks2 on the private bus, shaped as fx/udisks2-drives.json found the live one: drive
//! objects whose SMART values arrive with `GetManagedObjects`, and NVMe controllers that answer
//! `SmartGetAttributes` from their cache. It counts every method call it receives, including ones
//! it does not implement, so a test can prove a call was never made. Every call must carry
//! `NO_AUTO_START`, or the system bus would start a stopped udisksd; the fake checks that when it
//! is dropped, so every test that uses it checks it too.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use tokio::sync::watch;
use zbus::message::{Flags, Type};
use zbus::names::OwnedInterfaceName;
use zbus::{Connection, Message, MessageStream, OwnedObjectPath, OwnedVariant, Variant, interface};

use super::bus::{PrivateBus, next};

const UDISKS: &str = "org.freedesktop.UDisks2";
const UDISKS_PATH: &str = "/org/freedesktop/UDisks2";
const DRIVES_PATH: &str = "/org/freedesktop/UDisks2/drives/";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
const ATA: &str = "org.freedesktop.UDisks2.Drive.Ata";
pub const NVME: &str = "org.freedesktop.UDisks2.NVMe.Controller";
pub const NVME_SERIAL: &str = "2O412LA45EG7";
const NVME_WEAR_PCT: u8 = 3;
const NVME_HOURS: u64 = 12_345;
pub const ATA_SERIAL: &str = "WD-BLUE";
const ATA_HOURS: u64 = 20_000;
pub const NEVER_READ_SERIAL: &str = "NEVER-READ";
const SECONDS_PER_HOUR: u64 = 60 * 60;
/// Unix seconds, as UDisks2 reports when it last read a drive's SMART data.
const SMART_UPDATED: u64 = 1_792_631_967;
/// What UDisks2 reports for a drive it has not read yet.
const NEVER_UPDATED: u64 = 0;
// Another key of the live reply, so the feed must pick `percent_used` out by name.
const AVAIL_SPARE_PCT: u8 = 100;

fn path(name: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{DRIVES_PATH}{name}")).unwrap()
}

fn variant<'a>(value: impl Into<Variant<'a>>) -> OwnedVariant {
    OwnedVariant::try_from(value.into()).unwrap()
}

fn interface_name(name: &str) -> OwnedInterfaceName {
    OwnedInterfaceName::try_from(name).unwrap()
}

type Properties = HashMap<String, OwnedVariant>;
type ManagedObjects = HashMap<OwnedObjectPath, BTreeMap<OwnedInterfaceName, Properties>>;

/// Every method call the fake received, as (interface, member, whether the bus could have started
/// UDisks2 for it). A property read counts against the interface it reads, not against
/// `org.freedesktop.DBus.Properties`.
#[derive(Clone, Default)]
pub struct Calls(Arc<Mutex<Vec<(String, String, bool)>>>);

impl Calls {
    fn note(&self, message: &Message) {
        let header = message.header();
        let (Some(interface), Some(member)) = (header.interface(), header.member()) else {
            return;
        };
        let interface = if interface.as_str() == PROPERTIES {
            read_interface(message).unwrap_or_else(|| interface.to_string())
        } else {
            interface.to_string()
        };
        let auto_starts = !message
            .primary_header()
            .flags()
            .contains(Flags::NO_AUTO_START);
        self.0
            .lock()
            .unwrap()
            .push((interface, member.to_string(), auto_starts));
    }

    /// The members of every call that came without `NO_AUTO_START`.
    fn auto_starting(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(.., auto_starts)| *auto_starts)
            .map(|(_, member, _)| member.clone())
            .collect()
    }

    /// Calls of `member`, on any interface.
    pub fn calls(&self, member: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, called, _)| called == member)
            .count()
    }

    pub fn method_calls_on(&self, interface: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(on, ..)| on == interface)
            .count()
    }

    pub fn total_calls(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

/// The interface a `Get`, `GetAll` or `Set` names first in its body.
fn read_interface(message: &Message) -> Option<String> {
    let body = message.body();
    let member = message.header().member()?.to_string();
    match member.as_str() {
        "GetAll" => body.deserialize::<String>().ok(),
        "Get" => body
            .deserialize::<(String, String)>()
            .ok()
            .map(|(on, _)| on),
        "Set" => body
            .deserialize::<(String, String, OwnedVariant)>()
            .ok()
            .map(|(on, ..)| on),
        _ => None,
    }
}

#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "org.freedesktop.UDisks2.Error")]
enum UdisksError {
    #[zbus(error)]
    ZBus(zbus::Error),
    NotAuthorized(String),
}

/// Which of the fake's methods hold their reply back, as a stuck controller would.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Stuck {
    #[default]
    Nothing,
    ManagedObjects,
    Attributes,
}

/// Resolves once `method` is no longer the stuck one.
async fn until_unstuck(stuck: &watch::Sender<Stuck>, method: Stuck) {
    let mut now = stuck.subscribe();
    // The sender lives as long as the fake, so the wait never ends in an error the test cares about.
    let _ = now.wait_for(|stuck| *stuck != method).await;
}

struct FakeManager {
    objects: ManagedObjects,
    stuck: Arc<watch::Sender<Stuck>>,
}

#[interface(name = "org.freedesktop.DBus.ObjectManager")]
impl FakeManager {
    async fn get_managed_objects(&self) -> ManagedObjects {
        until_unstuck(&self.stuck, Stuck::ManagedObjects).await;
        self.objects.clone()
    }
}

struct FakeNvme {
    percent_used: u8,
    denied: bool,
    stuck: Arc<watch::Sender<Stuck>>,
}

#[interface(name = "org.freedesktop.UDisks2.NVMe.Controller")]
impl FakeNvme {
    async fn smart_get_attributes(
        &self,
        _options: HashMap<String, OwnedVariant>,
    ) -> Result<Properties, UdisksError> {
        until_unstuck(&self.stuck, Stuck::Attributes).await;
        if self.denied {
            return Err(UdisksError::NotAuthorized(
                "Not authorized to perform operation".into(),
            ));
        }
        Ok(HashMap::from([
            ("avail_spare".to_owned(), variant(AVAIL_SPARE_PCT)),
            ("percent_used".to_owned(), variant(self.percent_used)),
        ]))
    }
}

/// How the fake departs from its two drives: an NVMe SSD with 3 % wear and 12 345 h, and a SATA
/// disk with 20 000 h.
#[derive(Clone, Copy, Default)]
pub struct UdisksShape {
    /// Adds an NVMe drive UDisks2 has not read yet, as right after it starts.
    pub never_read_drive: bool,
    /// `SmartGetAttributes` answers `NotAuthorized`, as outside an active session.
    pub attributes_denied: bool,
}

pub struct FakeUdisks {
    connection: Connection,
    calls: Calls,
    stuck: Arc<watch::Sender<Stuck>>,
}

impl FakeUdisks {
    /// Serves the objects without taking UDisks2's name.
    pub async fn serve(bus: &PrivateBus, shape: UdisksShape) -> Self {
        let connection = bus.connect().await;
        let calls = Calls::default();
        let stuck = Arc::new(watch::Sender::new(Stuck::Nothing));
        let mut incoming = MessageStream::from(&connection);
        let noted = calls.clone();
        // Drained for as long as the connection lives, or its queue would fill and stall the fake.
        tokio::spawn(async move {
            while let Some(message) = next(&mut incoming).await {
                if let Ok(message) = message
                    && message.message_type() == Type::MethodCall
                {
                    noted.note(&message);
                }
            }
        });
        let server = connection.object_server();
        let mut objects = ManagedObjects::new();
        let mut nvmes = vec![(
            "XPG_GAMMIX_S70_BLADE_2O412LA45EG7",
            NVME_SERIAL,
            SMART_UPDATED,
            NVME_HOURS,
        )];
        if shape.never_read_drive {
            nvmes.push(("NEVER_READ", NEVER_READ_SERIAL, NEVER_UPDATED, 0));
        }
        for (name, serial, updated, hours) in nvmes {
            objects.insert(
                path(name),
                BTreeMap::from([
                    (interface_name(DRIVE), drive_properties(serial)),
                    (
                        interface_name(NVME),
                        HashMap::from([
                            ("State".to_owned(), variant("live")),
                            ("SmartUpdated".to_owned(), variant(updated)),
                            ("SmartPowerOnHours".to_owned(), variant(hours)),
                        ]),
                    ),
                ]),
            );
            let nvme = FakeNvme {
                percent_used: NVME_WEAR_PCT,
                denied: shape.attributes_denied,
                stuck: Arc::clone(&stuck),
            };
            server.at(path(name), nvme).await.unwrap();
        }
        objects.insert(
            path("WDC_WD10EZEX_WD_BLUE"),
            BTreeMap::from([
                (interface_name(DRIVE), drive_properties(ATA_SERIAL)),
                (
                    interface_name(ATA),
                    HashMap::from([
                        ("SmartUpdated".to_owned(), variant(SMART_UPDATED)),
                        (
                            "SmartPowerOnSeconds".to_owned(),
                            variant(ATA_HOURS * SECONDS_PER_HOUR),
                        ),
                    ]),
                ),
            ]),
        );
        server
            .at(
                UDISKS_PATH,
                FakeManager {
                    objects,
                    stuck: Arc::clone(&stuck),
                },
            )
            .await
            .unwrap();
        Self {
            connection,
            calls,
            stuck,
        }
    }

    pub async fn start(bus: &PrivateBus, shape: UdisksShape) -> Self {
        let udisks = Self::serve(bus, shape).await;
        udisks.claim().await;
        udisks
    }

    pub async fn claim(&self) {
        self.connection.request_name(UDISKS).await.unwrap();
    }

    pub async fn release(&self) {
        self.connection.release_name(UDISKS).await.unwrap();
    }

    /// Holds back every reply of `method` from now on, until another is stuck instead.
    pub fn stick(&self, method: Stuck) {
        self.stuck.send_replace(method);
    }

    pub fn calls(&self, member: &str) -> usize {
        self.calls.calls(member)
    }

    pub fn method_calls_on(&self, interface: &str) -> usize {
        self.calls.method_calls_on(interface)
    }

    pub fn total_calls(&self) -> usize {
        self.calls.total_calls()
    }
}

impl Drop for FakeUdisks {
    fn drop(&mut self) {
        // A second panic while a failing test unwinds would abort the run and hide its message.
        if !std::thread::panicking() {
            assert_eq!(
                self.calls.auto_starting(),
                Vec::<String>::new(),
                "calls the system bus could have started UDisks2 for"
            );
        }
    }
}

fn drive_properties(serial: &str) -> Properties {
    HashMap::from([("Serial".to_owned(), variant(serial.to_owned()))])
}
