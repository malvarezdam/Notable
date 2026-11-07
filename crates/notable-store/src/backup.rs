//! Archivo de respaldo en JSON.

use notable_core::Node;
use serde::{Deserialize, Serialize};

use crate::StoreError;

/// Valor del campo `format` de un respaldo de Notable.
pub const BACKUP_FORMAT: &str = "notable";
/// Versión del formato de respaldo que escribe esta app.
pub const BACKUP_VERSION: u32 = 1;

/// Contenido de un respaldo: todos los nodos, cada uno como en la base.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    /// Milisegundos desde 1970.
    pub exported_at: i64,
    pub nodes: Vec<Node>,
}

impl Backup {
    /// Respaldo con los nodos ordenados (padres antes que hijos, luego por posición), para que
    /// el archivo sea estable y legible.
    pub fn new(mut nodes: Vec<Node>, exported_at: i64) -> Self {
        nodes.sort_by_key(|n| (n.parent.is_some(), n.parent, n.position, n.id));
        Self {
            format: BACKUP_FORMAT.to_owned(),
            version: BACKUP_VERSION,
            exported_at,
            nodes,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("un respaldo siempre se serializa")
    }

    pub fn from_json(json: &str) -> Result<Self, StoreError> {
        let backup: Self = serde_json::from_str(json)
            .map_err(|e| StoreError::InvalidBackup(format!("no se pudo leer: {e}")))?;
        if backup.format != BACKUP_FORMAT {
            return Err(StoreError::InvalidBackup(
                "no es un respaldo de Notable".into(),
            ));
        }
        if backup.version > BACKUP_VERSION {
            return Err(StoreError::InvalidBackup(format!(
                "es de una versión más nueva de Notable (formato {}); actualiza la app",
                backup.version
            )));
        }
        Ok(backup)
    }
}
