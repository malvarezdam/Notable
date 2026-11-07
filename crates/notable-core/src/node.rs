use serde::{Deserialize, Serialize};

use crate::{Aggregation, GradeConfig, NodeId};

/// Un nodo del árbol. Carrera, año, semestre, ramo, evaluación… son solo nombres que pone el
/// usuario: todos son nodos.
///
/// Cada nodo se serializa por separado (JSON) con su `parent`, así se guarda como una fila en
/// SQLite y se sincroniza de a uno. Los campos de fases futuras se agregan con
/// `#[serde(default)]` para que los datos viejos sigan cargando, y los campos desconocidos se
/// ignoran para que una versión vieja de la app pueda leer datos de una más nueva.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    /// `None` si el nodo está en el nivel superior.
    #[serde(default)]
    pub parent: Option<NodeId>,
    pub name: String,
    /// Orden entre hermanos (menor primero). Lo mantiene el [`crate::Tree`].
    #[serde(default)]
    pub position: u32,
    /// Nota ingresada. Solo cuenta en las hojas; en un nodo con hijos se ignora.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<f64>,
    /// Peso dentro del padre, usado solo si el padre pondera. `None` = sin peso asignado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// Ancestro respecto al cual se expresa `weight` ("vale 20 % del ramo"). `None` = respecto
    /// al padre. Si deja de ser un ancestro (por ejemplo, tras mover el nodo), se usa el padre
    /// y el cálculo lo avisa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight_base: Option<NodeId>,
    /// Redondear la nota calculada de este nodo: `Some(true)` sí, `Some(false)` no, `None` =
    /// lo que diga [`GradeConfig::round_every_level`]. No afecta a las notas ingresadas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round_result: Option<bool>,
    /// Cómo se combinan los hijos de este nodo.
    #[serde(default)]
    pub aggregation: Aggregation,
    /// Configuración de nota propia. `None` = heredar del ancestro.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<GradeConfig>,
    /// Borrado lógico (se conserva para sincronizar el borrado).
    #[serde(default)]
    pub deleted: bool,
}

impl Node {
    /// Nodo nuevo con valores por defecto (promedio simple, sin nota, hereda configuración).
    pub fn new(name: impl Into<String>, parent: Option<NodeId>) -> Self {
        Self {
            id: NodeId::new(),
            parent,
            name: name.into(),
            position: 0,
            grade: None,
            weight: None,
            weight_base: None,
            round_result: None,
            aggregation: Aggregation::default(),
            config: None,
            deleted: false,
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}
