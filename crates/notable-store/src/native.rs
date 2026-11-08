//! Guardado en SQLite (app nativa).

use std::collections::HashMap;
use std::path::Path;

use notable_core::{Node, NodeId, Tree};
use rusqlite::{Connection, Transaction, params};

use crate::{Backup, Result, StoreError, now_ms, schema};

/// La base de datos local.
pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// Abre (o crea) la base en `path` y aplica las migraciones que falten.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        // WAL: escrituras rápidas y seguras ante cortes; `journal_mode` responde una fila.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        Self::init(conn)
    }

    /// Base en memoria (para pruebas).
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        schema::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// Carga todos los nodos guardados (incluidos los borrados) y arma el árbol.
    pub fn load_tree(&self) -> Result<Tree> {
        Ok(Tree::from_nodes(self.load_nodes()?)?)
    }

    /// Guarda los nodos que cambiaron desde el último guardado ([`Tree::take_dirty`]) y los deja
    /// pendientes de sincronizar. Devuelve cuántos se guardaron. Si falla, el árbol los
    /// conserva como cambiados para reintentar.
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

    /// Ids de los nodos con cambios sin sincronizar.
    pub fn dirty_ids(&self) -> Result<Vec<NodeId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM items WHERE dirty = 1 ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|id| {
            let id = id?;
            id.parse().map_err(|_| StoreError::InvalidId(id))
        })
        .collect()
    }

    /// Momento (milisegundos desde 1970) de la última modificación guardada de un nodo.
    pub fn updated_at(&self, id: NodeId) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT updated_at FROM items WHERE id = ?1")?;
        let mut rows = stmt.query_map([id.to_string()], |row| row.get(0))?;
        Ok(rows.next().transpose()?)
    }

    // ---------------------------------------------------------------- respaldo

    /// Respaldo en JSON de todo lo guardado (incluidos los borrados, para poder restaurarlos).
    pub fn export_json(&self) -> Result<String> {
        let backup = Backup::new(self.load_nodes()?, now_ms());
        Ok(backup.to_json())
    }

    /// Escribe el respaldo en un archivo.
    pub fn export_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, self.export_json()?)?;
        Ok(())
    }

    /// Importa un respaldo **sin borrar nada**: los nodos del respaldo reemplazan a los que
    /// tienen el mismo id y los que solo existen aquí se conservan. Devuelve el árbol resultante.
    /// Si el resultado no fuera un árbol válido, no se cambia nada.
    pub fn import_json(&mut self, json: &str) -> Result<Tree> {
        let backup = Backup::from_json(json)?;
        let mut merged: HashMap<NodeId, Node> =
            self.load_nodes()?.into_iter().map(|n| (n.id, n)).collect();
        for node in &backup.nodes {
            merged.insert(node.id, node.clone());
        }
        let tree = Tree::from_nodes(merged.into_values())?;
        self.write_nodes(backup.nodes.iter())?;
        Ok(tree)
    }

    /// Importa un respaldo desde un archivo (ver [`Store::import_json`]).
    pub fn import_from_file(&mut self, path: impl AsRef<Path>) -> Result<Tree> {
        let json = std::fs::read_to_string(path)?;
        self.import_json(&json)
    }

    // ---------------------------------------------------------------- internos

    pub(crate) fn load_nodes(&self) -> Result<Vec<Node>> {
        let mut stmt = self.conn.prepare("SELECT id, data FROM items")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (id, data) = row?;
            Node::from_json(&data).map_err(|error| StoreError::CorruptRow { id, error })
        })
        .collect()
    }

    /// Escribe nodos en una sola transacción, con la hora actual y marcados como pendientes.
    fn write_nodes<'a>(&mut self, nodes: impl Iterator<Item = &'a Node>) -> Result<()> {
        let tx = self.conn.transaction()?;
        let now = now_ms();
        for node in nodes {
            upsert(&tx, node, now)?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// Inserta o actualiza una fila. `updated_at` nunca retrocede (si el reloj se atrasó, avanza
/// en 1 ms), para que "gana el cambio más reciente" funcione al sincronizar.
fn upsert(tx: &Transaction<'_>, node: &Node, now: i64) -> Result<()> {
    let data = node.to_json().expect("un nodo siempre se serializa");
    tx.execute(
        "INSERT INTO items (id, parent_id, data, updated_at, deleted, dirty)
         VALUES (?1, ?2, ?3, ?4, ?5, 1)
         ON CONFLICT (id) DO UPDATE SET
             parent_id  = excluded.parent_id,
             data       = excluded.data,
             updated_at = max(excluded.updated_at, items.updated_at + 1),
             deleted    = excluded.deleted,
             dirty      = 1",
        params![
            node.id.to_string(),
            node.parent.map(|p| p.to_string()),
            data,
            now,
            node.deleted,
        ],
    )?;
    Ok(())
}
