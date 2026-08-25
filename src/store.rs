//! SQLite persistence for Detectic: stores network-map snapshots, derives a
//! stable pseudonymized device id (HMAC-SHA256 of the MAC), and reports what
//! changed between consecutive captures.

use crate::crypto::pseudonymize;
use crate::model::{MapDiff, NetworkMap};
use rusqlite::params;
use rusqlite::OptionalExtension;
use std::path::Path;

pub struct Store {
    conn: rusqlite::Connection,
    secret: Vec<u8>,
}

impl Store {
    pub fn open<P: AsRef<Path>>(path: P, secret: &[u8]) -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY,
                captured_at INTEGER NOT NULL,
                raw_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS devices (
                id INTEGER PRIMARY KEY,
                snapshot_id INTEGER NOT NULL,
                device_key TEXT NOT NULL,
                pseudonym TEXT NOT NULL,
                hostname TEXT,
                ip TEXT,
                mac TEXT,
                rssi INTEGER,
                standard TEXT,
                onemesh_stack TEXT,
                assoc_time INTEGER,
                radio_mac TEXT,
                source TEXT,
                FOREIGN KEY(snapshot_id) REFERENCES snapshots(id)
            );
            CREATE INDEX IF NOT EXISTS idx_devices_key ON devices(device_key);
            CREATE INDEX IF NOT EXISTS idx_devices_pseudo ON devices(pseudonym);",
        )?;
        Ok(Self {
            conn,
            secret: secret.to_vec(),
        })
    }

    /// Persist a snapshot. Returns the new snapshot id.
    pub fn save(&mut self, map: &NetworkMap) -> rusqlite::Result<i64> {
        let tx = self.conn.transaction()?;
        let raw = serde_json::to_string(map).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
        tx.execute(
            "INSERT INTO snapshots (captured_at, raw_json) VALUES (?1, ?2)",
            params![map.captured_at, raw],
        )?;
        let snap_id = tx.last_insert_rowid();

        for d in &map.devices {
            let key = d.identity();
            let pseudo = pseudonymize(&self.secret, d.mac.as_deref().unwrap_or(&key));
            tx.execute(
                "INSERT INTO devices
                    (snapshot_id, device_key, pseudonym, hostname, ip, mac, rssi,
                     standard, onemesh_stack, assoc_time, radio_mac, source)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    snap_id,
                    key,
                    pseudo,
                    d.hostname,
                    d.ip,
                    d.mac,
                    d.rssi,
                    d.standard,
                    d.onemesh_stack,
                    d.assoc_time,
                    d.radio_mac,
                    d.source,
                ],
            )?;
        }
        tx.commit()?;
        Ok(snap_id)
    }

    /// Load the most recent snapshot before `before_snap_id` (or the latest when
    /// `None`), to diff against.
    pub fn latest_before(
        &self,
        before_snap_id: Option<i64>,
    ) -> rusqlite::Result<Option<NetworkMap>> {
        let row: Option<(i64, String)> = match before_snap_id {
            Some(id) => self
                .conn
                .query_row(
                    "SELECT id, raw_json FROM snapshots WHERE id < ?1 ORDER BY id DESC LIMIT 1",
                    params![id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?,
            None => self
                .conn
                .query_row(
                    "SELECT id, raw_json FROM snapshots ORDER BY id DESC LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?,
        };
        match row {
            Some((_, json)) => Ok(Some(serde_json::from_str(&json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?)),
            None => Ok(None),
        }
    }

    /// Compute the diff between the previous stored snapshot and `current`.
    pub fn diff_with_previous(&self, current: &NetworkMap) -> rusqlite::Result<MapDiff> {
        let prev = self.latest_before(None)?;
        Ok(match prev {
            Some(p) => p.diff(current),
            None => MapDiff::default(),
        })
    }

    /// Count distinct pseudonyms ever seen.
    pub fn distinct_devices(&self) -> rusqlite::Result<usize> {
        let n: i64 =
            self.conn
                .query_row("SELECT COUNT(DISTINCT pseudonym) FROM devices", [], |r| {
                    r.get(0)
                })?;
        Ok(n as usize)
    }

    /// Count stored snapshots.
    pub fn snapshot_count(&self) -> rusqlite::Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Milestone M4 local aggregation: per-device statistics across all snapshots.
    /// RSSI aggregates are `None` when the device was never observed with signal.
    pub fn device_aggregates(&self) -> rusqlite::Result<Vec<DeviceStats>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.pseudonym, d.hostname, d.mac, d.source,
                    MIN(s.captured_at)            AS first_seen,
                    MAX(s.captured_at)            AS last_seen,
                    COUNT(*)                      AS observations,
                    CAST(ROUND(AVG(d.rssi)) AS INTEGER) AS avg_rssi,
                    MIN(d.rssi)                   AS min_rssi,
                    MAX(d.rssi)                   AS max_rssi
             FROM devices d
             JOIN snapshots s ON d.snapshot_id = s.id
             GROUP BY d.pseudonym
             ORDER BY last_seen DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DeviceStats {
                pseudonym: r.get(0)?,
                hostname: r.get(1)?,
                mac: r.get(2)?,
                source: r.get(3)?,
                first_seen: r.get(4)?,
                last_seen: r.get(5)?,
                observations: r.get(6)?,
                avg_rssi: r.get(7)?,
                min_rssi: r.get(8)?,
                max_rssi: r.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

/// Per-device aggregate across stored snapshots (Milestone M4).
#[derive(Debug, Clone)]
pub struct DeviceStats {
    pub pseudonym: String,
    pub hostname: Option<String>,
    pub mac: Option<String>,
    pub source: Option<String>,
    pub first_seen: i64,
    pub last_seen: i64,
    pub observations: i64,
    pub avg_rssi: Option<i64>,
    pub min_rssi: Option<i64>,
    pub max_rssi: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::Store;
    use crate::model::{Device, NetworkMap};

    fn dev(mac: &str, rssi: Option<i64>, ts: i64) -> NetworkMap {
        NetworkMap {
            captured_at: ts,
            raw: Default::default(),
            devices: vec![Device {
                hostname: Some("h".into()),
                ip: Some("10.0.0.1".into()),
                mac: Some(mac.into()),
                rssi,
                standard: None,
                onemesh_stack: None,
                assoc_time: None,
                radio_mac: None,
                source: Some("wifi".into()),
                tx_rate: None,
                rx_rate: None,
                noise: None,
                signal_level: None,
                max_link_rate: None,
                interface: None,
                ipv6: None,
                client_type: None,
                active: None,
            }],
        }
    }

    #[test]
    fn restart_recovers_previous_snapshot_and_emits_events() {
        let mut s = Store::open(":memory:", b"secret").unwrap();

        // First snapshot: one device.
        let mut d1 = dev("AA:BB:CC:DD:EE:FF", Some(-50), 1_000);
        d1.devices[0].hostname = Some("phone".into());
        d1.devices[0].ip = Some("10.0.0.1".into());
        s.save(&d1).unwrap();

        // Re-open is not needed for :memory:, but diff_with_previous loads the
        // latest row, simulating a restart reading the database.
        let mut d2 = dev("AA:BB:CC:DD:EE:FF", Some(-55), 2_000);
        d2.devices[0].hostname = Some("phone".into());
        d2.devices[0].ip = Some("10.0.0.2".into()); // IP changed
        d2.devices.push(Device {
            hostname: Some("laptop".into()),
            ip: Some("10.0.0.3".into()),
            mac: Some("11:22:33:44:55:66".into()),
            rssi: Some(-60),
            standard: Some("ax".into()),
            onemesh_stack: None,
            assoc_time: None,
            radio_mac: None,
            source: Some("wifi".into()),
            tx_rate: None,
            rx_rate: None,
            noise: None,
            signal_level: None,
            max_link_rate: None,
            interface: None,
            ipv6: None,
            client_type: None,
            active: None,
        });

        let diff = s.diff_with_previous(&d2).unwrap();
        let events = crate::events::diff_to_events(&diff, d2.captured_at, |id| {
            crate::pseudonymize(b"secret", id)
        });

        // One device changed IP, one new device joined.
        assert_eq!(events.len(), 2);
        let updated = events
            .iter()
            .find(|e| e.kind == crate::events::EventKind::DeviceUpdated)
            .unwrap();
        assert!(updated.changed_fields.contains(&"ip".into()));
        assert!(events
            .iter()
            .any(|e| e.kind == crate::events::EventKind::DeviceJoined));

        s.save(&d2).unwrap();
        assert_eq!(s.snapshot_count().unwrap(), 2);
    }

    #[test]
    fn pseudonymization_isolates_sensors() {
        let mut s = Store::open(":memory:", b"secret-a").unwrap();
        let mut s2 = Store::open(":memory:", b"secret-b").unwrap();

        let mac = "AA:BB:CC:DD:EE:FF";
        let p1 = crate::pseudonymize(b"secret-a", mac);
        let p2 = crate::pseudonymize(b"secret-b", mac);
        assert_ne!(p1, p2);

        s.save(&dev(mac, Some(-50), 1_000)).unwrap();
        s2.save(&dev(mac, Some(-50), 1_000)).unwrap();

        let r1 = s.device_aggregates().unwrap();
        let r2 = s2.device_aggregates().unwrap();
        assert_eq!(r1[0].pseudonym, p1);
        assert_eq!(r2[0].pseudonym, p2);
    }

    #[test]
    fn aggregates_two_snapshots() {
        let mut s = Store::open(":memory:", b"secret").unwrap();
        s.save(&dev("AA:BB:CC:DD:EE:FF", Some(-50), 1_000)).unwrap();
        s.save(&dev("AA:BB:CC:DD:EE:FF", Some(-60), 2_000)).unwrap();
        // a second, never-seen device with no rssi
        s.save(&dev("11:22:33:44:55:66", None, 2_000)).unwrap();

        let rows = s.device_aggregates().unwrap();
        assert_eq!(rows.len(), 2);

        // device1 has 2 observations with rssi -50/-60
        let a = rows.iter().find(|d| d.avg_rssi == Some(-55)).unwrap();
        assert_eq!(a.observations, 2);
        assert_eq!(a.first_seen, 1_000);
        assert_eq!(a.last_seen, 2_000);
        assert_eq!(a.avg_rssi, Some(-55));
        assert_eq!(a.min_rssi, Some(-60));
        assert_eq!(a.max_rssi, Some(-50));

        // device2 was seen once and never reported an RSSI
        let b = rows.iter().find(|d| d.avg_rssi.is_none()).unwrap();
        assert_eq!(b.observations, 1);
        assert_eq!(b.avg_rssi, None);
    }
}
