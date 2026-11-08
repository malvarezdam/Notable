//! La fila que se sincroniza, común a todos los guardados.

use notable_core::{Node, NodeId};

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
