//! Offline GeoIP and ASN lookups in MaxMind DB files.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use maxminddb::geoip2::{self, city::Location};
use maxminddb::{Mmap, PathElement, Reader};
use tracing::{debug, warn};

pub const DBIP_CITY: &str = "dbip-city-lite.mmdb";
pub const DBIP_ASN: &str = "dbip-asn-lite.mmdb";
/// Fedora's `geolite2-city` package, used while no DB-IP city database is there.
pub const FALLBACK_CITY: &str = "/usr/share/GeoIP/GeoLite2-City.mmdb";
const DBIP_ATTRIBUTION: &str = "IP GEOLOCATION BY DB-IP";
const GEOLITE2_ATTRIBUTION: &str = "GEOLITE2 DATA BY MAXMIND";
const NO_ATTRIBUTION: &str = "";
const XDG_DATA_HOME_ENV: &str = "XDG_DATA_HOME";
const HOME_ENV: &str = "HOME";
const DATA_HOME_SUBDIR: &str = ".local/share";
const GEOIP_SUBDIR: &str = "ghost-shell/geoip";
const LOCATION: &str = "location";

pub trait Geo: Send + Sync {
    /// The latitude and longitude of `ip`, in degrees.
    fn city(&self, ip: IpAddr) -> Option<(f32, f32)>;
    /// The autonomous system `ip` belongs to, and its organisation, `""` when the database names
    /// none.
    fn asn(&self, ip: IpAddr) -> Option<(u32, String)>;
    /// The licence notice the city database asks to be shown, `""` while none is loaded.
    fn attribution(&self) -> &'static str;
    /// Picks up databases that appeared, changed or went away since the last call.
    fn refresh(&self);
}

/// `geoip_dir`, or `$XDG_DATA_HOME/ghost-shell/geoip` when it is `""`.
pub fn geoip_dir(geoip_dir: &str) -> PathBuf {
    if !geoip_dir.is_empty() {
        return PathBuf::from(geoip_dir);
    }
    geoip_dir_in(env::var_os(XDG_DATA_HOME_ENV), env::var_os(HOME_ENV))
}

fn geoip_dir_in(xdg_data_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    xdg_data_home
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home.unwrap_or_default()).join(DATA_HOME_SUBDIR))
        .join(GEOIP_SUBDIR)
}

/// DB-IP's City and ASN Lite databases, memory-mapped, so only the pages lookups touch become
/// resident.
pub struct MmdbGeo {
    databases: Mutex<Databases>,
}

struct Databases {
    city: Database,
    fallback_city: Database,
    asn: Database,
}

impl MmdbGeo {
    /// Maps the databases in `dir` that exist now; [`Geo::refresh`] picks up later ones.
    pub fn open(dir: &Path) -> Self {
        let geo = Self {
            databases: Mutex::new(Databases {
                city: Database::new(dir.join(DBIP_CITY)),
                fallback_city: Database::new(PathBuf::from(FALLBACK_CITY)),
                asn: Database::new(dir.join(DBIP_ASN)),
            }),
        };
        geo.refresh();
        geo
    }

    fn lock(&self) -> MutexGuard<'_, Databases> {
        self.databases
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Geo for MmdbGeo {
    fn city(&self, ip: IpAddr) -> Option<(f32, f32)> {
        let databases = self.lock();
        let reader = databases
            .city
            .reader
            .as_ref()
            .or(databases.fallback_city.reader.as_ref())?;
        // Decoding only `location` leaves the names and subdivisions of the record unread.
        let location: Location = reader
            .lookup(ip)
            .ok()?
            .decode_path(&[PathElement::Key(LOCATION)])
            .ok()??;
        Some((location.latitude? as f32, location.longitude? as f32))
    }

    fn asn(&self, ip: IpAddr) -> Option<(u32, String)> {
        let databases = self.lock();
        let reader = databases.asn.reader.as_ref()?;
        let record: geoip2::Asn = reader.lookup(ip).ok()?.decode().ok()??;
        Some((
            record.autonomous_system_number?,
            record
                .autonomous_system_organization
                .unwrap_or_default()
                .to_owned(),
        ))
    }

    fn attribution(&self) -> &'static str {
        let databases = self.lock();
        if databases.city.reader.is_some() {
            DBIP_ATTRIBUTION
        } else if databases.fallback_city.reader.is_some() {
            GEOLITE2_ATTRIBUTION
        } else {
            NO_ATTRIBUTION
        }
    }

    fn refresh(&self) {
        let mut databases = self.lock();
        databases.city.refresh();
        databases.fallback_city.refresh();
        databases.asn.refresh();
    }
}

/// One file, mapped while it exists, and the modification time it was mapped at.
struct Database {
    path: PathBuf,
    mapped_at: Option<SystemTime>,
    reader: Option<Reader<Mmap>>,
}

impl Database {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            mapped_at: None,
            reader: None,
        }
    }

    fn refresh(&mut self) {
        let modified = fs::metadata(&self.path)
            .and_then(|metadata| metadata.modified())
            .ok();
        if modified == self.mapped_at {
            return;
        }
        self.mapped_at = modified;
        self.reader = modified.and_then(|_| self.map());
    }

    fn map(&self) -> Option<Reader<Mmap>> {
        // SAFETY: the mapping is undefined behaviour only if the file is truncated or written in
        // place while mapped. scripts/fetch-geoip.sh and Fedora's packages both replace the file
        // with a rename, which leaves the mapped inode untouched; the new file has a new mtime,
        // and is mapped on the next refresh.
        let reader = unsafe { Reader::open_mmap(&self.path) };
        match reader {
            Ok(reader) => {
                debug!(path = %self.path.display(), "mapped a GeoIP database");
                Some(reader)
            }
            Err(error) => {
                warn!(%error, path = %self.path.display(), "cannot open a GeoIP database");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_dir_follows_xdg_data_home_then_home() {
        assert_eq!(
            geoip_dir_in(Some("/data".into()), Some("/home/u".into())),
            PathBuf::from("/data/ghost-shell/geoip")
        );
        assert_eq!(
            geoip_dir_in(Some("".into()), Some("/home/u".into())),
            PathBuf::from("/home/u/.local/share/ghost-shell/geoip")
        );
        assert_eq!(geoip_dir("/srv/geoip"), PathBuf::from("/srv/geoip"));
    }

    #[test]
    fn missing_databases_answer_nothing_and_attribute_nothing() {
        let dir = std::env::temp_dir().join("ghostd-no-geoip-here");
        let mut geo = MmdbGeo::open(&dir);
        // The fallback may exist on the machine running the tests; drop it to test the empty case.
        geo.databases.get_mut().unwrap().fallback_city = Database::new(dir.join(DBIP_CITY));
        let ip = "1.1.1.1".parse().unwrap();
        assert_eq!((geo.city(ip), geo.asn(ip)), (None, None));
        assert_eq!(geo.attribution(), "");
    }
}
