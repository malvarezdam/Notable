//! Pruebas de la sincronización con un servidor en memoria que se comporta como el de
//! `supabase/schema.sql` (gana el `updated_at` más reciente; cada escritura recibe hora de
//! servidor).

use std::collections::HashMap;

use notable_core::{NodeId, Tree};
use notable_store::{Store, SyncRow};
use notable_sync::{Remote, RemoteRow, SyncError, bind_account, sync};

#[derive(Default)]
struct Server {
    rows: HashMap<NodeId, RemoteRow>,
    clock: i64,
    offline: bool,
}

impl Server {
    fn tick(&mut self) -> i64 {
        // La hora del servidor siempre avanza (un lote comparte la misma hora, como en Postgres).
        self.clock += 1_000_000;
        self.clock
    }
}

impl Remote for Server {
    fn pull(&mut self, since: Option<i64>) -> Result<Vec<RemoteRow>, SyncError> {
        if self.offline {
            return Err(SyncError::Offline("sin red".into()));
        }
        let mut rows: Vec<RemoteRow> = self
            .rows
            .values()
            .filter(|r| since.is_none_or(|s| r.server_updated_at > s))
            .cloned()
            .collect();
        rows.sort_by_key(|r| (r.server_updated_at, r.row.id));
        Ok(rows)
    }

    fn push(&mut self, rows: &[SyncRow]) -> Result<(), SyncError> {
        if self.offline {
            return Err(SyncError::Offline("sin red".into()));
        }
        let now = self.tick();
        for row in rows {
            let older = self
                .rows
                .get(&row.id)
                .is_some_and(|old| row.updated_at < old.row.updated_at);
            if !older {
                self.rows.insert(
                    row.id,
                    RemoteRow {
                        row: row.clone(),
                        server_updated_at: now,
                    },
                );
            }
        }
        Ok(())
    }
}

/// Un dispositivo: su base y su árbol en memoria, como la app.
struct Device {
    store: Store,
    tree: Tree,
}

impl Device {
    fn new() -> Self {
        let mut store = Store::open_in_memory().unwrap();
        bind_account(&mut store, "user-1").unwrap();
        let tree = store.load_tree().unwrap();
        Self { store, tree }
    }

    fn edit(&mut self, f: impl FnOnce(&mut Tree)) {
        f(&mut self.tree);
        self.store.save(&mut self.tree).unwrap();
    }

    fn sync(&mut self, server: &mut Server) -> notable_sync::Report {
        let report = sync(&mut self.store, server).unwrap();
        self.tree = self.store.load_tree().unwrap();
        report
    }

    fn names(&self, parent: Option<NodeId>) -> Vec<String> {
        let mut names: Vec<String> = self.tree.children(parent).map(|n| n.name.clone()).collect();
        names.sort();
        names
    }
}

#[test]
fn changes_reach_the_other_device() {
    let mut server = Server::default();
    let mut pc = Device::new();
    let mut phone = Device::new();

    let mut course = None;
    pc.edit(|t| {
        let c = t.create(None, "Cálculo").unwrap();
        let e = t.create(Some(c), "Certamen 1").unwrap();
        t.set_grade(e, Some(5.5)).unwrap();
        course = Some(c);
    });
    let report = pc.sync(&mut server);
    assert_eq!(report.pushed, 2);
    assert!(pc.store.pending().unwrap().is_empty());

    let report = phone.sync(&mut server);
    assert_eq!(report.applied, 2);
    assert_eq!(phone.names(None), ["Cálculo"]);
    let eval = phone.tree.evaluate();
    assert_eq!(eval.grade(course.unwrap()), Some(5.5));

    // Y de vuelta.
    phone.edit(|t| {
        t.create(course, "Certamen 2").unwrap();
    });
    phone.sync(&mut server);
    pc.sync(&mut server);
    assert_eq!(pc.names(course), ["Certamen 1", "Certamen 2"]);
}

#[test]
fn newest_change_wins_per_node_and_nothing_is_lost() {
    let mut server = Server::default();
    let mut pc = Device::new();
    let mut phone = Device::new();
    let mut ids = (NodeId::new(), NodeId::new());
    pc.edit(|t| {
        ids.0 = t.create(None, "Ramo A").unwrap();
        ids.1 = t.create(None, "Ramo B").unwrap();
    });
    pc.sync(&mut server);
    phone.sync(&mut server);

    // Los dos trabajan sin conexión: el teléfono cambia A, el PC cambia A después y crea C.
    phone.edit(|t| t.rename(ids.0, "A (teléfono)").unwrap());
    std::thread::sleep(std::time::Duration::from_millis(5));
    pc.edit(|t| {
        t.rename(ids.0, "A (PC)").unwrap();
        t.create(None, "Ramo C").unwrap();
    });
    phone.edit(|t| t.rename(ids.1, "B (teléfono)").unwrap());

    phone.sync(&mut server);
    pc.sync(&mut server);
    phone.sync(&mut server);

    let expected = ["A (PC)", "B (teléfono)", "Ramo C"];
    assert_eq!(pc.names(None), expected);
    assert_eq!(phone.names(None), expected);
}

#[test]
fn deletions_sync_and_can_be_undone() {
    let mut server = Server::default();
    let mut pc = Device::new();
    let mut phone = Device::new();
    let mut id = NodeId::new();
    pc.edit(|t| id = t.create(None, "Ramo").unwrap());
    pc.sync(&mut server);
    phone.sync(&mut server);

    pc.edit(|t| t.delete(id).unwrap());
    pc.sync(&mut server);
    phone.sync(&mut server);
    assert!(phone.names(None).is_empty());
    assert!(phone.tree.get(id).unwrap().deleted);

    phone.edit(|t| t.restore(id).unwrap());
    phone.sync(&mut server);
    pc.sync(&mut server);
    assert_eq!(pc.names(None), ["Ramo"]);
}

#[test]
fn first_login_merges_local_data() {
    let mut server = Server::default();
    let mut pc = Device::new();
    pc.edit(|t| {
        t.create(None, "Del PC").unwrap();
    });
    pc.sync(&mut server);

    // Un teléfono usado sin cuenta, con datos propios, entra a la misma cuenta.
    let store = Store::open_in_memory().unwrap();
    let mut phone = Device {
        tree: store.load_tree().unwrap(),
        store,
    };
    phone.edit(|t| {
        t.create(None, "Del teléfono").unwrap();
    });
    bind_account(&mut phone.store, "user-1").unwrap();
    phone.sync(&mut server);
    pc.sync(&mut server);

    let both = ["Del PC", "Del teléfono"];
    assert_eq!(phone.names(None), both);
    assert_eq!(pc.names(None), both);
}

#[test]
fn switching_account_marks_everything_pending() {
    let mut server = Server::default();
    let mut pc = Device::new();
    pc.edit(|t| {
        t.create(None, "Ramo").unwrap();
    });
    pc.sync(&mut server);
    assert!(pc.store.pending().unwrap().is_empty());

    bind_account(&mut pc.store, "user-1").unwrap();
    assert!(
        pc.store.pending().unwrap().is_empty(),
        "misma cuenta: nada cambia"
    );

    bind_account(&mut pc.store, "user-2").unwrap();
    assert_eq!(pc.store.pending().unwrap().len(), 1);
}

#[test]
fn offline_keeps_everything_pending() {
    let mut server = Server {
        offline: true,
        ..Server::default()
    };
    let mut pc = Device::new();
    pc.edit(|t| {
        t.create(None, "Ramo").unwrap();
    });
    let error = sync(&mut pc.store, &mut server).unwrap_err();
    assert!(error.is_transient());
    assert_eq!(pc.store.pending().unwrap().len(), 1);
    assert_eq!(pc.store.load_tree().unwrap().len(), 1);

    server.offline = false;
    assert_eq!(pc.sync(&mut server).pushed, 1);
}

#[test]
fn change_made_while_uploading_stays_pending() {
    let mut pc = Device::new();
    let mut id = NodeId::new();
    pc.edit(|t| id = t.create(None, "Ramo").unwrap());
    let pending = pc.store.pending().unwrap();
    // Mientras "se sube", el usuario cambia el nodo.
    std::thread::sleep(std::time::Duration::from_millis(2));
    pc.edit(|t| t.rename(id, "Ramo nuevo").unwrap());
    notable_sync::finish_push(&mut pc.store, &pending, 0).unwrap();
    assert_eq!(pc.store.pending().unwrap().len(), 1);
}

#[test]
fn stale_remote_rows_do_not_overwrite_newer_local() {
    let mut server = Server::default();
    let mut pc = Device::new();
    let mut id = NodeId::new();
    pc.edit(|t| id = t.create(None, "Viejo").unwrap());
    pc.sync(&mut server);
    pc.edit(|t| t.rename(id, "Nuevo").unwrap());
    // Volver a bajar lo mismo (por el margen del cursor) no pisa el cambio local.
    let rows = server.pull(None).unwrap();
    notable_sync::finish_pull(&mut pc.store, &rows).unwrap();
    assert_eq!(pc.store.load_tree().unwrap().get(id).unwrap().name, "Nuevo");
}
