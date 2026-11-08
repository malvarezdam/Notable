//! Guardado para el navegador: mismas reglas que la base SQLite, sobre un texto JSON.
//!
//! Cada escritura reemplaza el texto completo de una vez, así un cierre a medias nunca deja los
//! datos partidos. Antes de cada operación se vuelve a leer, porque otra pestaña pudo cambiarlos.
//! El lugar donde queda el texto es un [`Backend`], lo que permite probar todo sin navegador.

use std::collections::{BTreeMap, HashMap};

use notable_core::{Node, NodeId, Tree};
use serde::{Deserialize, Serialize};

use crate::{Backup, Result, StoreError, SyncRow, now_ms};

/// Versión del formato guardado.
const FORMAT_VERSION: u32 = 1;

/// Donde queda guardado el texto.
pub trait Backend {
    /// El texto guardado, o `None` si todavía no hay nada.
    fn read(&self) -> Result<Option<String>>;
    /// Reemplaza el texto guardado.
    fn write(&mut self, data: &str) -> Result<()>;
}

#[derive(Clone, Serialize, Deserialize)]
struct Row {
    parent_id: Option<String>,
    data: String,
    updated_at: i64,
    deleted: bool,
    dirty: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    version: u32,
    items: BTreeMap<String, Row>,
    meta: BTreeMap<String, String>,
}

/// Guardado local sobre un [`Backend`].
pub struct KvStore<B> {
    backend: B,
}

impl<B: Backend> KvStore<B> {
    /// Abre el guardado y comprueba que se pueda leer.
    pub fn new(backend: B) -> Result<Self> {
        let store = Self { backend };
        store.read()?;
        Ok(store)
    }

    /// Carga todos los nodos guardados (incluidos los borrados) y arma el árbol.
    pub fn load_tree(&self) -> Result<Tree> {
        Ok(Tree::from_nodes(nodes_of(&self.read()?)?)?)
    }

    /// Guarda los nodos que cambiaron y los deja pendientes de sincronizar. Si falla, el árbol
    /// los conserva como cambiados para reintentar.
    pub fn save(&mut self, tree: &mut Tree) -> Result<usize> {
        let ids = tree.take_dirty();
        if ids.is_empty() {
            return Ok(0);
        }
        let result = self.write_nodes(ids.iter().filter_map(|id| tree.get(*id)));
        if result.is_err() {
            tree.mark_dirty(ids.iter().copied());
        }
        result.map(|()| ids.len())
    }

    /// Respaldo en JSON de todo lo guardado (incluidos los borrados).
    pub fn export_json(&self) -> Result<String> {
        Ok(Backup::new(nodes_of(&self.read()?)?, now_ms()).to_json())
    }

    /// Importa un respaldo sin borrar nada. Si el resultado no fuera un árbol válido, no se
    /// cambia nada.
    pub fn import_json(&mut self, json: &str) -> Result<Tree> {
        let backup = Backup::from_json(json)?;
        let mut merged: HashMap<NodeId, Node> = nodes_of(&self.read()?)?
            .into_iter()
            .map(|n| (n.id, n))
            .collect();
        for node in &backup.nodes {
            merged.insert(node.id, node.clone());
        }
        let tree = Tree::from_nodes(merged.into_values())?;
        self.write_nodes(backup.nodes.iter())?;
        Ok(tree)
    }

    /// Filas con cambios sin subir.
    pub fn pending(&self) -> Result<Vec<SyncRow>> {
        self.read()?
            .items
            .iter()
            .filter(|(_, row)| row.dirty)
            .map(|(id, row)| {
                Ok(SyncRow {
                    id: parse_id(id)?,
                    parent_id: row.parent_id.as_deref().map(parse_id).transpose()?,
                    data: row.data.clone(),
                    updated_at: row.updated_at,
                    deleted: row.deleted,
                })
            })
            .collect()
    }

    /// Marca como subidas las filas que siguen en la versión que se subió. Si el usuario cambió
    /// una fila mientras se subía, queda pendiente para la próxima vez.
    pub fn mark_synced(&mut self, rows: &[(NodeId, i64)]) -> Result<()> {
        let mut saved = self.read()?;
        for (id, updated_at) in rows {
            if let Some(row) = saved.items.get_mut(&id.to_string())
                && row.updated_at == *updated_at
            {
                row.dirty = false;
            }
        }
        self.write(&saved)
    }

    /// Marca todo como pendiente de subir.
    pub fn mark_all_dirty(&mut self) -> Result<()> {
        let mut saved = self.read()?;
        for row in saved.items.values_mut() {
            row.dirty = true;
        }
        self.write(&saved)
    }

    /// Aplica filas del servidor: gana el `updated_at` más reciente. Si el resultado no forma un
    /// árbol válido, no se cambia nada. Devuelve cuántas filas cambiaron aquí.
    pub fn apply_remote(&mut self, rows: &[SyncRow]) -> Result<usize> {
        let mut saved = self.read()?;
        let mut nodes: HashMap<NodeId, Node> =
            nodes_of(&saved)?.into_iter().map(|n| (n.id, n)).collect();

        let mut applied = 0;
        for row in rows {
            let key = row.id.to_string();
            let newer = saved
                .items
                .get(&key)
                .is_none_or(|local| row.updated_at > local.updated_at);
            if !newer {
                continue;
            }
            let node = Node::from_json(&row.data).map_err(|error| StoreError::CorruptRow {
                id: key.clone(),
                error,
            })?;
            if node.id != row.id {
                return Err(StoreError::InvalidId(key));
            }
            nodes.insert(node.id, node);
            saved.items.insert(
                key,
                Row {
                    parent_id: row.parent_id.map(|p| p.to_string()),
                    data: row.data.clone(),
                    updated_at: row.updated_at,
                    deleted: row.deleted,
                    dirty: false,
                },
            );
            applied += 1;
        }
        if applied == 0 {
            return Ok(0);
        }
        // Comprobar antes de escribir.
        Tree::from_nodes(nodes.into_values())?;
        self.write(&saved)?;
        Ok(applied)
    }

    /// Lee un dato suelto.
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self.read()?.meta.get(key).cloned())
    }

    /// Guarda (o borra, con `None`) un dato suelto.
    pub fn set_meta(&mut self, key: &str, value: Option<&str>) -> Result<()> {
        let mut saved = self.read()?;
        match value {
            Some(v) => saved.meta.insert(key.to_owned(), v.to_owned()),
            None => saved.meta.remove(key),
        };
        self.write(&saved)
    }

    fn read(&self) -> Result<Saved> {
        let Some(text) = self.backend.read()? else {
            return Ok(Saved {
                version: FORMAT_VERSION,
                items: BTreeMap::new(),
                meta: BTreeMap::new(),
            });
        };
        let saved: Saved = serde_json::from_str(&text)
            .map_err(|e| StoreError::Storage(format!("datos dañados: {e}")))?;
        if saved.version > FORMAT_VERSION {
            return Err(StoreError::NewerSchema {
                found: saved.version,
                supported: FORMAT_VERSION,
            });
        }
        Ok(saved)
    }

    fn write(&mut self, saved: &Saved) -> Result<()> {
        let text = serde_json::to_string(saved).expect("el guardado siempre se serializa");
        self.backend.write(&text)
    }

    /// `updated_at` nunca retrocede (si el reloj se atrasó, avanza en 1 ms), para que "gana el
    /// cambio más reciente" funcione al sincronizar.
    fn write_nodes<'a>(&mut self, nodes: impl Iterator<Item = &'a Node>) -> Result<()> {
        let mut saved = self.read()?;
        let now = now_ms();
        for node in nodes {
            let key = node.id.to_string();
            let updated_at = saved
                .items
                .get(&key)
                .map_or(now, |old| now.max(old.updated_at + 1));
            saved.items.insert(
                key,
                Row {
                    parent_id: node.parent.map(|p| p.to_string()),
                    data: node.to_json().expect("un nodo siempre se serializa"),
                    updated_at,
                    deleted: node.deleted,
                    dirty: true,
                },
            );
        }
        self.write(&saved)
    }
}

fn nodes_of(saved: &Saved) -> Result<Vec<Node>> {
    saved
        .items
        .iter()
        .map(|(id, row)| {
            Node::from_json(&row.data).map_err(|error| StoreError::CorruptRow {
                id: id.clone(),
                error,
            })
        })
        .collect()
}

fn parse_id(id: &str) -> Result<NodeId> {
    id.parse().map_err(|_| StoreError::InvalidId(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    /// Guardado en memoria, compartible para simular reabrir o una segunda pestaña.
    #[derive(Clone, Default)]
    struct Memory {
        text: Rc<RefCell<Option<String>>>,
        full: Rc<RefCell<bool>>,
    }

    impl Backend for Memory {
        fn read(&self) -> Result<Option<String>> {
            Ok(self.text.borrow().clone())
        }

        fn write(&mut self, data: &str) -> Result<()> {
            if *self.full.borrow() {
                return Err(StoreError::Storage("lleno".into()));
            }
            *self.text.borrow_mut() = Some(data.to_owned());
            Ok(())
        }
    }

    fn course() -> (KvStore<Memory>, Memory, Tree, NodeId) {
        let memory = Memory::default();
        let mut store = KvStore::new(memory.clone()).unwrap();
        let mut tree = store.load_tree().unwrap();
        let id = tree.create(None, "Física").unwrap();
        assert_eq!(store.save(&mut tree).unwrap(), 1);
        (store, memory, tree, id)
    }

    fn remote(node: &Node, updated_at: i64) -> SyncRow {
        SyncRow::from_node(node, updated_at)
    }

    #[test]
    fn what_is_saved_survives_reopening() {
        let (mut store, memory, mut tree, id) = course();
        let c1 = tree.create(Some(id), "C1").unwrap();
        tree.set_grade(c1, Some(5.5)).unwrap();
        store.save(&mut tree).unwrap();

        let reopened = KvStore::new(memory).unwrap().load_tree().unwrap();
        assert_eq!(reopened.get(c1).unwrap().grade, Some(5.5));
        assert_eq!(reopened.get(id).unwrap().name, "Física");
    }

    #[test]
    fn changes_stay_pending_until_uploaded_in_that_version() {
        let (mut store, _, mut tree, id) = course();
        let pending = store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        let uploaded = pending[0].updated_at;

        // Cambió mientras se subía: sigue pendiente.
        tree.rename(id, "Química").unwrap();
        store.save(&mut tree).unwrap();
        store.mark_synced(&[(id, uploaded)]).unwrap();
        let pending = store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].updated_at > uploaded, "nunca retrocede");

        store.mark_synced(&[(id, pending[0].updated_at)]).unwrap();
        assert!(store.pending().unwrap().is_empty());
        store.mark_all_dirty().unwrap();
        assert_eq!(store.pending().unwrap().len(), 1);
    }

    #[test]
    fn newest_version_wins_when_applying_remote_rows() {
        let (mut store, _, tree, id) = course();
        let local = store.pending().unwrap()[0].updated_at;
        let mut node = tree.get(id).unwrap().clone();
        node.name = "Remoto".into();

        assert_eq!(store.apply_remote(&[remote(&node, local - 1)]).unwrap(), 0);
        assert_eq!(store.load_tree().unwrap().get(id).unwrap().name, "Física");

        assert_eq!(store.apply_remote(&[remote(&node, local + 1)]).unwrap(), 1);
        assert_eq!(store.load_tree().unwrap().get(id).unwrap().name, "Remoto");
        assert!(
            store.pending().unwrap().is_empty(),
            "lo que llega no se sube"
        );
    }

    #[test]
    fn invalid_remote_rows_change_nothing() {
        let (mut store, memory, _, _) = course();
        let before = memory.text.borrow().clone();
        let orphan = Node::new("Huérfano", Some(NodeId::new()));
        assert!(store.apply_remote(&[remote(&orphan, i64::MAX)]).is_err());
        assert_eq!(*memory.text.borrow(), before);
    }

    #[test]
    fn a_second_tab_does_not_lose_the_first_ones_changes() {
        let (mut first, memory, mut tree, id) = course();
        let mut second = KvStore::new(memory).unwrap();
        second.set_meta("clave", Some("valor")).unwrap();
        tree.rename(id, "Química").unwrap();
        first.save(&mut tree).unwrap();

        assert_eq!(first.meta("clave").unwrap().as_deref(), Some("valor"));
        assert_eq!(second.load_tree().unwrap().get(id).unwrap().name, "Química");
        second.set_meta("clave", None).unwrap();
        assert_eq!(first.meta("clave").unwrap(), None);
    }

    #[test]
    fn full_storage_keeps_the_change_for_retrying() {
        let (mut store, memory, mut tree, id) = course();
        tree.rename(id, "Química").unwrap();
        *memory.full.borrow_mut() = true;
        assert!(store.save(&mut tree).is_err());
        assert!(tree.is_dirty(id));
        *memory.full.borrow_mut() = false;
        assert_eq!(store.save(&mut tree).unwrap(), 1);
    }

    #[test]
    fn backup_round_trip_is_compatible_and_merges() {
        let (mut store, _, _, id) = course();
        let json = store.export_json().unwrap();

        let (mut other, _, _, other_id) = course();
        let merged = other.import_json(&json).unwrap();
        assert!(merged.get(id).is_some());
        assert!(merged.get(other_id).is_some(), "no borra lo que ya había");
        assert!(store.import_json("no es json").is_err());
    }

    #[test]
    fn damaged_or_newer_data_is_not_overwritten() {
        let memory = Memory::default();
        *memory.text.borrow_mut() = Some("{dañado".into());
        assert!(matches!(
            KvStore::new(memory.clone()),
            Err(StoreError::Storage(_))
        ));
        assert_eq!(memory.text.borrow().as_deref(), Some("{dañado"));

        *memory.text.borrow_mut() = Some(r#"{"version":99,"items":{},"meta":{}}"#.into());
        assert!(matches!(
            KvStore::new(memory),
            Err(StoreError::NewerSchema { .. })
        ));
    }
}
