use std::fmt;

use crate::NodeId;

/// Configuración de nota inválida.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    NotFinite,
    EmptyScale,
    PassOutOfScale,
    TooManyDecimals,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NotFinite => "la escala tiene valores no numéricos",
            Self::EmptyScale => "la nota mínima debe ser menor que la máxima",
            Self::PassOutOfScale => "la nota de aprobación debe estar dentro de la escala",
            Self::TooManyDecimals => "demasiados decimales",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for ConfigError {}

/// Error de una operación sobre el árbol.
#[derive(Clone, Debug, PartialEq)]
pub enum TreeError {
    /// No existe un nodo con ese id.
    NotFound(NodeId),
    /// Ya existe un nodo con ese id (al cargar).
    DuplicateId(NodeId),
    /// El nodo apunta a un padre que no existe (al cargar).
    MissingParent { node: NodeId, parent: NodeId },
    /// El movimiento o la carga formaría un ciclo.
    Cycle(NodeId),
    /// La operación no se puede hacer sobre un nodo borrado (o dentro de uno).
    Deleted(NodeId),
    /// Solo las hojas (nodos sin hijos) tienen nota propia.
    NotALeaf(NodeId),
    /// El peso solo puede expresarse respecto a un ancestro del nodo.
    NotAnAncestor { node: NodeId, base: NodeId },
    /// El nombre está vacío.
    EmptyName,
    /// Nota o peso no numérico (NaN o infinito), o peso negativo.
    InvalidNumber,
    /// Configuración de nota inválida.
    Config(ConfigError),
    /// Regla opcional inválida (número no válido, reemplazo de sí mismo o de algo que no es un
    /// hermano, o reemplazo y eximición a la vez en el mismo nodo).
    InvalidRule,
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "no existe el nodo {id}"),
            Self::DuplicateId(id) => write!(f, "id repetido: {id}"),
            Self::MissingParent { node, parent } => {
                write!(f, "el nodo {node} apunta a un padre inexistente {parent}")
            }
            Self::Cycle(id) => write!(f, "el nodo {id} quedaría dentro de sí mismo"),
            Self::Deleted(id) => write!(f, "el nodo {id} está borrado"),
            Self::NotALeaf(id) => write!(f, "el nodo {id} tiene hijos; su nota se calcula"),
            Self::NotAnAncestor { node, base } => {
                write!(f, "{base} no es un ancestro de {node}")
            }
            Self::EmptyName => f.write_str("el nombre no puede estar vacío"),
            Self::InvalidNumber => f.write_str("número inválido"),
            Self::Config(e) => write!(f, "configuración de nota inválida: {e}"),
            Self::InvalidRule => f.write_str("regla inválida"),
        }
    }
}

impl std::error::Error for TreeError {}

impl From<ConfigError> for TreeError {
    fn from(e: ConfigError) -> Self {
        Self::Config(e)
    }
}
