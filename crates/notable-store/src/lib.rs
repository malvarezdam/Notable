//! `notable-store`: guardado local de Notable.
//!
//! En la app nativa cada nodo es una fila de SQLite con el nodo en JSON ([`Store`]). En el
//! navegador no hay SQLite: [`KvStore`] guarda lo mismo, con las mismas reglas, en el
//! almacenamiento del navegador. La app carga el árbol completo al iniciar y, después de cada
//! cambio, llama a `save`, que escribe los nodos modificados al instante y los deja marcados
//! para sincronizar. También se puede exportar e importar un respaldo en JSON.

mod backup;
mod error;
mod kv;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
mod native_sync;
#[cfg(not(target_arch = "wasm32"))]
mod schema;
mod sync;
#[cfg(target_arch = "wasm32")]
mod web;

pub use backup::{BACKUP_FORMAT, BACKUP_VERSION, Backup};
pub use error::StoreError;
pub use kv::{Backend, KvStore};
#[cfg(not(target_arch = "wasm32"))]
pub use native::Store;
#[cfg(not(target_arch = "wasm32"))]
pub use schema::SCHEMA_VERSION;
pub use sync::SyncRow;
#[cfg(target_arch = "wasm32")]
pub use web::{STORAGE_KEY, Store};

pub type Result<T> = std::result::Result<T, StoreError>;

/// Milisegundos desde 1970 según el reloj del dispositivo.
#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Milisegundos desde 1970 según el reloj del dispositivo.
#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}
