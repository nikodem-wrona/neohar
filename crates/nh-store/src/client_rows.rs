//! The client devices of a core: `clients`. The core notes when each client
//! was last seen, and it removes the windows of a client that did not connect for 30 days.

use rusqlite::params;
use ulid::Ulid;

use crate::{Store, StoreError, Tx};

/// A client device: its device id, its name (the host name) and when it was last seen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientRow {
    pub device: Ulid,
    pub name: String,
    /// The last attach or detach of the client, in unix milliseconds.
    pub last_seen: i64,
}

fn read_clients(conn: &rusqlite::Connection) -> Result<Vec<ClientRow>, StoreError> {
    let mut statement =
        conn.prepare("SELECT device, name, last_seen FROM clients ORDER BY device")?;
    let mut rows = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let device: String = row.get(0)?;
        out.push(ClientRow {
            device: device
                .parse()
                .map_err(|_| StoreError::BadValue(format!("client {device:?} is no ULID")))?,
            name: row.get(1)?,
            last_seen: row.get(2)?,
        });
    }
    Ok(out)
}

impl Tx<'_> {
    /// Notes that a client was seen now: it attached or detached.
    pub fn touch_client(&self, row: &ClientRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO clients (device, name, last_seen) VALUES (?1, ?2, ?3)
             ON CONFLICT (device) DO UPDATE SET name = excluded.name,
                 last_seen = excluded.last_seen",
            params![row.device.to_string(), row.name, row.last_seen],
        )?;
        Ok(())
    }

    /// Removes a client. Its windows are not touched: the caller closes them first.
    pub fn delete_client(&self, device: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "DELETE FROM clients WHERE device = ?1",
            [device.to_string()],
        )?;
        Ok(())
    }

    /// Every client that the core has seen, in the order of the device ids.
    pub fn clients(&self) -> Result<Vec<ClientRow>, StoreError> {
        read_clients(self.sql())
    }
}

impl Store {
    /// Every client that the core has seen, in the order of the device ids.
    pub fn clients(&self) -> Result<Vec<ClientRow>, StoreError> {
        read_clients(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WindowRow;

    fn device(n: u128) -> Ulid {
        Ulid::from(n)
    }

    #[test]
    fn a_client_is_noted_again_and_removed() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1_000, |tx| {
                tx.touch_client(&ClientRow {
                    device: device(1),
                    name: "laptop".to_owned(),
                    last_seen: 1_000,
                })?;
                tx.touch_client(&ClientRow {
                    device: device(2),
                    name: "studio".to_owned(),
                    last_seen: 1_000,
                })?;
                tx.touch_client(&ClientRow {
                    device: device(1),
                    name: "laptop-2".to_owned(),
                    last_seen: 5_000,
                })
            })
            .expect("the clients are written");
        assert_eq!(
            store.clients().expect("the clients"),
            [
                ClientRow {
                    device: device(1),
                    name: "laptop-2".to_owned(),
                    last_seen: 5_000,
                },
                ClientRow {
                    device: device(2),
                    name: "studio".to_owned(),
                    last_seen: 1_000,
                },
            ]
        );
        store
            .tx(6_000, |tx| tx.delete_client(device(2)))
            .expect("the client is removed");
        assert_eq!(store.clients().expect("the clients").len(), 1);
    }

    #[test]
    fn a_window_keeps_the_device_of_its_client() {
        let mut store = Store::open_in_memory().expect("a store");
        let window = |n: u128, client: Option<Ulid>| WindowRow {
            id: Ulid::from(n),
            bounds: None,
            tab_by_workspace: serde_json::json!({}),
            visible_panes: Vec::new(),
            focused_pane: None,
            popout_pane: None,
            position: u32::try_from(n).unwrap_or(0),
            client,
        };
        store
            .tx(1_000, |tx| {
                tx.put_window(&window(10, Some(device(1))))?;
                tx.put_window(&window(11, None))
            })
            .expect("the windows are written");
        let rows = store.load_layout().expect("the layout").windows;
        assert_eq!(rows[0].client, Some(device(1)));
        assert_eq!(rows[1].client, None);
        // A client adopts the window without one.
        store
            .tx(2_000, |tx| tx.put_window(&window(11, Some(device(2)))))
            .expect("the window is written");
        let rows = store.load_layout().expect("the layout").windows;
        assert_eq!(rows[1].client, Some(device(2)));
    }
}
