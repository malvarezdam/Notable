//! Lo que necesita la sincronización de la base local: filas pendientes, aplicar lo que llega
//! del servidor y datos sueltos (`meta`). La comunicación con el servidor está en
//! `notable-sync`; aquí no hay red.

use std::collections::HashMap;

use notable_core::{Node, NodeId, Tree};
use rusqlite::{OptionalExtension, params};

use crate::{Result, Store, StoreError};

/// Una fila tal como se sincroniza: el nodo en JSON y cuándo lo cambió el usuario.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncRow {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    /// El nodo serializado ([`Node::to_json`]).
    pub data: String,
    /// Milisegundos desde 1970 de la última modificación hecha por el usuario.
    pub updated_at: i64,
    pub deleted: bool,
}

impl SyncRow {
    /// Fila a partir de un nodo y su momento de modificación.
    pub fn from_node(node: &Node, updated_at: i64) -> Self {
        Self {
            id: node.id,
            parent_id: node.parent,
            data: node.to_json().expect("un nodo siempre se serializa"),
            updated_at,
            deleted: node.deleted,
        }
    }
}

impl Store {
    /// Filas con cambios sin subir.
    pub fn pending(&self) -> Result<Vec<SyncRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parent_id, data, updated_at, deleted FROM items WHERE dirty = 1 ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (id, parent, data, updated_at, deleted) = row?;
            Ok(SyncRow {
                id: parse_id(&id)?,
                parent_id: parent.as_deref().map(parse_id).transpose()?,
                data,
                updated_at,
                deleted,
            })
        })
        .collect()
    }

    /// Marca como subidas las filas que siguen en la versión que se subió (mismo `updated_at`).
    /// Si el usuario cambió una fila mientras se subía, queda pendiente para la próxima vez.
    pub fn mark_synced(&mut self, rows: &[(NodeId, i64)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare("UPDATE items SET dirty = 0 WHERE id = ?1 AND updated_at = ?2")?;
            for (id, updated_at) in rows {
                stmt.execute(params![id.to_string(), updated_at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Marca todo como pendiente de subir (por ejemplo, al entrar con otra cuenta: lo de este
    /// dispositivo se fusiona con lo de la cuenta).
    pub fn mark_all_dirty(&mut self) -> Result<()> {
        self.conn.execute("UPDATE items SET dirty = 1", [])?;
        Ok(())
    }

    /// Aplica filas que llegaron del servidor. Gana la versión con el `updated_at` más reciente;
    /// si la local es igual o más nueva, se conserva (y si está pendiente, se subirá). Nada se
    /// borra de verdad: los borrados también son filas.
    ///
    /// Si el resultado no forma un árbol válido, no se cambia nada. Devuelve cuántas filas
    /// cambiaron aquí.
    pub fn apply_remote(&mut self, rows: &[SyncRow]) -> Result<usize> {
        let local = self.versions()?;
        let mut nodes: HashMap<NodeId, Node> =
            self.load_nodes()?.into_iter().map(|n| (n.id, n)).collect();

        let mut winners = Vec::new();
        for row in rows {
            let newer = local.get(&row.id).is_none_or(|&t| row.updated_at > t);
            if !newer {
                continue;
            }
            let node = Node::from_json(&row.data).map_err(|error| StoreError::CorruptRow {
                id: row.id.to_string(),
                error,
            })?;
            if node.id != row.id {
                return Err(StoreError::InvalidId(row.id.to_string()));
            }
            nodes.insert(node.id, node);
            winners.push(row);
        }
        if winners.is_empty() {
            return Ok(0);
        }
        // Comprobar antes de escribir.
        Tree::from_nodes(nodes.into_values())?;

        let tx = self.conn.transaction()?;
        for row in &winners {
            tx.execute(
                "INSERT INTO items (id, parent_id, data, updated_at, deleted, dirty)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0)
                 ON CONFLICT (id) DO UPDATE SET
                     parent_id  = excluded.parent_id,
                     data       = excluded.data,
                     updated_at = excluded.updated_at,
                     deleted    = excluded.deleted,
                     dirty      = 0",
                params![
                    row.id.to_string(),
                    row.parent_id.map(|p| p.to_string()),
                    row.data,
                    row.updated_at,
                    row.deleted,
                ],
            )?;
        }
        tx.commit()?;
        Ok(winners.len())
    }

    /// Lee un dato suelto.
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// Guarda (o borra, con `None`) un dato suelto.
    pub fn set_meta(&mut self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self.conn.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, v],
            )?,
            None => self
                .conn
                .execute("DELETE FROM meta WHERE key = ?1", [key])?,
        };
        Ok(())
    }

    /// `updated_at` de cada fila local.
    fn versions(&self) -> Result<HashMap<NodeId, i64>> {
        let mut stmt = self.conn.prepare("SELECT id, updated_at FROM items")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.map(|row| {
            let (id, t) = row?;
            Ok((parse_id(&id)?, t))
        })
        .collect()
    }
}

fn parse_id(id: &str) -> Result<NodeId> {
    id.parse().map_err(|_| StoreError::InvalidId(id.to_owned()))
}
