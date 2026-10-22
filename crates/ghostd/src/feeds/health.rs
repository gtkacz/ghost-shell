//! The `disks.health` topic: each drive's wear and power-on hours, from UDisks2 on the system bus.
//!
//! Nothing runs until some client wants the topic, and everything stops once none does. While it
//! is wanted, UDisks2 is read at once, again every 10 min (spec §12), and again in full whenever
//! its bus name gains or loses an owner. Every read is published; an absent UDisks2 reads as no
//! drives.
//!
//! Only what UDisks2 has already cached is read, so no drive is ever woken: the SMART properties
//! that `GetManagedObjects` carries, and `SmartGetAttributes` on NVMe controllers, which answers
//! from the cache. `SmartUpdate` is never called, and no method on an ATA drive is, since that
//! could spin up a sleeping disk.
//!
//! Due times are wall-clock seconds, because tokio's clock stands still while the machine is
//! suspended: a reading from before a night's sleep is refreshed within a minute of waking.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::pending;
use std::sync::Arc;
use std::time::Duration;

use ghost_proto::{DiskHealth, DriveHealth, topics};
use tokio::time::sleep;
use tracing::{debug, warn};
use zbus::fdo::{self, NameOwnerChangedStream};
use zbus::names::OwnedInterfaceName;
use zbus::proxy::{self, CacheProperties, MethodFlags};
use zbus::wire::DynamicDeserialize;
use zbus::{Connection, DynamicType, OwnedObjectPath, OwnedVariant, Proxy, Variant};

use super::{WallClock, drain, next, unix_now_s, until_unwanted, until_wanted};
use crate::server::{Feed, FeedContext, Publisher};

const UDISKS: &str = "org.freedesktop.UDisks2";
const UDISKS_PATH: &str = "/org/freedesktop/UDisks2";
const OBJECT_MANAGER_INTERFACE: &str = "org.freedesktop.DBus.ObjectManager";
const GET_MANAGED_OBJECTS: &str = "GetManagedObjects";
const SMART_GET_ATTRIBUTES: &str = "SmartGetAttributes";
const DRIVE_INTERFACE: &str = "org.freedesktop.UDisks2.Drive";
const ATA_INTERFACE: &str = "org.freedesktop.UDisks2.Drive.Ata";
const NVME_INTERFACE: &str = "org.freedesktop.UDisks2.NVMe.Controller";
const SERIAL: &str = "Serial";
/// Unix seconds of UDisks2's last SMART read of the drive, 0 while it has none.
const SMART_UPDATED: &str = "SmartUpdated";
const SMART_POWER_ON_HOURS: &str = "SmartPowerOnHours";
const SMART_POWER_ON_SECONDS: &str = "SmartPowerOnSeconds";
const PERCENT_USED: &str = "percent_used";
const SECONDS_PER_HOUR: u64 = 60 * 60;
const HEALTH_EVERY_S: u64 = 10 * 60;
// Tokio's timers stop during a suspend and nothing reports the resume, so while the topic is
// wanted the feed looks at the wall clock at least this often.
const WALL_CLOCK_CHECK: Duration = Duration::from_secs(60);

type Properties = HashMap<String, OwnedVariant>;
type Interfaces = BTreeMap<OwnedInterfaceName, Properties>;

/// Reads UDisks2 on the system bus, which is connected to only once the topic is first wanted.
pub fn feed() -> Feed {
    Feed::new(topics::DISKS_HEALTH, |mut context| async move {
        until_wanted(&mut context.demand, topics::DISKS_HEALTH).await;
        match Connection::system().await {
            Ok(bus) => run(bus, Arc::new(unix_now_s), context).await,
            Err(error) => {
                warn!(%error, "cannot connect to the system bus; disks.health has no drives");
                context
                    .publisher
                    .publish(topics::DISKS_HEALTH, &DiskHealth::default());
                pending::<()>().await;
            }
        }
    })
}

/// As [`feed`], reading UDisks2 on `bus` and the wall clock from `clock`.
pub fn feed_on(bus: Connection, clock: WallClock) -> Feed {
    Feed::new(topics::DISKS_HEALTH, move |context| {
        run(bus, clock, context)
    })
}

async fn run(bus: Connection, clock: WallClock, mut context: FeedContext) {
    let mut reader = Reader::default();
    loop {
        until_wanted(&mut context.demand, topics::DISKS_HEALTH).await;
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::DISKS_HEALTH) => {}
            () = read_forever(&bus, &clock, &mut reader, &context.publisher) => {}
        }
    }
}

/// Reads and publishes now, then whenever a read falls due or UDisks2 comes or goes.
async fn read_forever(
    bus: &Connection,
    clock: &WallClock,
    reader: &mut Reader,
    publisher: &Publisher,
) {
    let mut owners = listen_for_owners(bus)
        .await
        .inspect_err(|error| {
            warn!(%error, "cannot watch for UDisks2; disks.health reads it only on its cadence");
        })
        .ok();
    loop {
        if let Some(owners) = &mut owners {
            // A read covers every change queued before it starts.
            drain(owners);
        }
        let drives = reader.read(bus).await;
        let read_s = clock();
        publisher.publish(topics::DISKS_HEALTH, &DiskHealth { drives });
        until_due(owners.as_mut(), clock, read_s).await;
    }
}

/// Must be listening before the first read, so that no change of owner is missed.
async fn listen_for_owners(bus: &Connection) -> zbus::Result<NameOwnerChangedStream> {
    fdo::DBusProxy::builder(bus)
        .cache_properties(CacheProperties::No)
        .build()
        .await?
        .receive_name_owner_changed_with_args(&[(0, UDISKS)])
        .await
}

/// Resolves once [`HEALTH_EVERY_S`] wall-clock seconds have passed since `read_s`, or UDisks2
/// gains or loses an owner.
async fn until_due(
    mut owners: Option<&mut NameOwnerChangedStream>,
    clock: &WallClock,
    read_s: u64,
) {
    loop {
        let now_s = clock();
        // A clock set back before the last read would otherwise hold the next one back by as much
        // as it moved.
        let wait_s = if now_s < read_s {
            0
        } else {
            (read_s + HEALTH_EVERY_S).saturating_sub(now_s)
        };
        if wait_s == 0 {
            return;
        }
        tokio::select! {
            () = owner_change(owners.as_deref_mut()) => return,
            () = sleep(Duration::from_secs(wait_s).min(WALL_CLOCK_CHECK)) => {}
        }
    }
}

/// Never resolves without the stream, or once it has ended with the bus.
async fn owner_change(owners: Option<&mut NameOwnerChangedStream>) {
    if let Some(owners) = owners
        && next(owners).await.is_some()
    {
        return;
    }
    pending().await
}

/// Reads UDisks2, remembering which drives it has already logged a failure for.
#[derive(Default)]
struct Reader {
    logged: HashSet<String>,
}

impl Reader {
    /// Every drive UDisks2 knows that has a serial, by serial. An absent UDisks2 reads as none.
    async fn read(&mut self, bus: &Connection) -> Vec<DriveHealth> {
        let objects = match managed_objects(bus).await {
            Ok(objects) => objects,
            Err(error) => {
                debug!(%error, "UDisks2 is not answering; no drive health");
                return Vec::new();
            }
        };
        let mut drives = Vec::new();
        for (path, interfaces) in &objects {
            let Some(serial) = interface(interfaces, DRIVE_INTERFACE)
                .and_then(|drive| text(drive, SERIAL))
                .filter(|serial| !serial.is_empty())
            else {
                continue;
            };
            let health = if let Some(nvme) = interface(interfaces, NVME_INTERFACE) {
                self.nvme_health(bus, path, serial, nvme).await
            } else if let Some(ata) = interface(interfaces, ATA_INTERFACE) {
                ata_health(serial, ata)
            } else {
                DriveHealth {
                    serial,
                    wear_pct: None,
                    power_on_hours: None,
                }
            };
            drives.push(health);
        }
        drives.sort_by(|a, b| a.serial.cmp(&b.serial));
        drives
    }

    async fn nvme_health(
        &mut self,
        bus: &Connection,
        path: &OwnedObjectPath,
        serial: String,
        nvme: &Properties,
    ) -> DriveHealth {
        if !ever_read(nvme) {
            return DriveHealth {
                serial,
                wear_pct: None,
                power_on_hours: None,
            };
        }
        let wear_pct = match percent_used(bus, path).await {
            Ok(wear_pct) => wear_pct,
            Err(error) => {
                if self.logged.insert(serial.clone()) {
                    warn!(%error, serial, "UDisks2 does not give this drive's wear; it stays unknown");
                }
                None
            }
        };
        DriveHealth {
            power_on_hours: number(nvme, SMART_POWER_ON_HOURS)
                .and_then(|hours| u32::try_from(hours).ok()),
            serial,
            wear_pct,
        }
    }
}

async fn managed_objects(bus: &Connection) -> zbus::Result<HashMap<OwnedObjectPath, Interfaces>> {
    call_udisks(
        bus,
        UDISKS_PATH,
        OBJECT_MANAGER_INTERFACE,
        GET_MANAGED_OBJECTS,
        &(),
    )
    .await
}

/// `percent_used` from the controller's cached SMART log, `None` if the log lacks it.
async fn percent_used(bus: &Connection, path: &OwnedObjectPath) -> zbus::Result<Option<u8>> {
    let options: HashMap<&str, Variant<'_>> = HashMap::new();
    let attributes: Properties = call_udisks(
        bus,
        path.as_str(),
        NVME_INTERFACE,
        SMART_GET_ATTRIBUTES,
        &(options,),
    )
    .await?;
    Ok(attributes
        .get(PERCENT_USED)
        .and_then(|value| value.downcast_ref().ok()))
}

/// Calls `method` on UDisks2 without letting the bus start it: the system bus can activate
/// udisksd, whose start-up probes every drive, so a stopped UDisks2 answers `ServiceUnknown` here
/// and is read once it gains an owner.
async fn call_udisks<B, R>(
    bus: &Connection,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<R>
where
    B: serde::Serialize + DynamicType,
    R: for<'d> DynamicDeserialize<'d>,
{
    let reply = proxy::Builder::<Proxy<'_>>::new(bus)
        .destination(UDISKS)
        .path(path)
        .interface(interface)
        .cache_properties(CacheProperties::No)
        .build()
        .await?
        .call_with_flags(method, MethodFlags::NO_AUTO_START, body)
        .await?;
    Ok(reply.expect("a call not flagged NO_REPLY_EXPECTED always has a reply"))
}

/// Hours only, from the properties: ATA drives report no wear.
fn ata_health(serial: String, ata: &Properties) -> DriveHealth {
    let power_on_hours = if ever_read(ata) {
        number(ata, SMART_POWER_ON_SECONDS)
            .and_then(|seconds| u32::try_from(seconds / SECONDS_PER_HOUR).ok())
    } else {
        None
    };
    DriveHealth {
        serial,
        wear_pct: None,
        power_on_hours,
    }
}

fn ever_read(properties: &Properties) -> bool {
    number(properties, SMART_UPDATED).is_some_and(|updated_s| updated_s > 0)
}

fn interface<'a>(interfaces: &'a Interfaces, name: &str) -> Option<&'a Properties> {
    interfaces
        .iter()
        .find(|(interface, _)| interface.as_str() == name)
        .map(|(_, properties)| properties)
}

fn text(properties: &Properties, name: &str) -> Option<String> {
    let value: &str = properties.get(name)?.downcast_ref().ok()?;
    Some(value.trim().to_owned())
}

fn number(properties: &Properties, name: &str) -> Option<u64> {
    properties.get(name)?.downcast_ref().ok()
}
