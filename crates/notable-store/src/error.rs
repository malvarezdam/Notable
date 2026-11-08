use std::fmt;

use notable_core::TreeError;

/// Error del guardado local.
#[derive(Debug)]
pub enum StoreError {
    /// Error de SQLite.
    #[cfg(not(target_arch = "wasm32"))]
    Sqlite(rusqlite::Error),
    /// El almacenamiento del navegador falló (lleno o bloqueado).
    Storage(String),
    /// Error al leer o escribir un archivo de respaldo.
    Io(std::io::Error),
    /// Una fila guardada no es un nodo válido.
    CorruptRow {
        id: String,
        error: serde_json::Error,
    },
    /// Una fila tiene un id que no es un UUID.
    InvalidId(String),
    /// Los nodos guardados (o importados) no forman un árbol válido.
    Tree(TreeError),
    /// La base de datos es de una versión más nueva de la app.
    NewerSchema { found: u32, supported: u32 },
    /// El respaldo no es un archivo de Notable, o es de una versión más nueva.
    InvalidBackup(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Sqlite(e) => write!(f, "error de la base de datos: {e}"),
            Self::Storage(e) => write!(f, "no se pudo usar el almacenamiento del navegador: {e}"),
            Self::Io(e) => write!(f, "error de archivo: {e}"),
            Self::CorruptRow { id, error } => {
                write!(f, "el nodo guardado {id} está dañado: {error}")
            }
            Self::InvalidId(id) => write!(f, "id guardado inválido: {id}"),
            Self::Tree(e) => write!(f, "los datos no forman un árbol válido: {e}"),
            Self::NewerSchema { found, supported } => write!(
                f,
                "la base de datos es de una versión más nueva de Notable (esquema {found}, esta \
                 versión entiende hasta {supported}); actualiza la app"
            ),
            Self::InvalidBackup(msg) => write!(f, "respaldo inválido: {msg}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Sqlite(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::CorruptRow { error, .. } => Some(error),
            Self::Tree(e) => Some(e),
            Self::NewerSchema { .. }
            | Self::InvalidBackup(_)
            | Self::InvalidId(_)
            | Self::Storage(_) => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<TreeError> for StoreError {
    fn from(e: TreeError) -> Self {
        Self::Tree(e)
    }
}
