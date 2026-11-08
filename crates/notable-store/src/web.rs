//! Guardado en el almacenamiento del navegador (`localStorage`).

use crate::{Backend, KvStore, Result, StoreError};

/// Clave donde queda todo lo guardado.
pub const STORAGE_KEY: &str = "notable.datos";

/// El guardado local en el navegador.
pub type Store = KvStore<LocalStorage>;

/// `localStorage` del navegador.
pub struct LocalStorage;

fn storage() -> Result<web_sys::Storage> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or_else(|| StoreError::Storage("este navegador no permite guardar datos".into()))
}

impl Backend for LocalStorage {
    fn read(&self) -> Result<Option<String>> {
        storage()?
            .get_item(STORAGE_KEY)
            .map_err(|_| StoreError::Storage("no se pudo leer".into()))
    }

    fn write(&mut self, data: &str) -> Result<()> {
        storage()?
            .set_item(STORAGE_KEY, data)
            .map_err(|_| StoreError::Storage("no hay espacio disponible".into()))
    }
}

impl KvStore<LocalStorage> {
    /// Abre el guardado del navegador.
    pub fn open_browser() -> Result<Self> {
        Self::new(LocalStorage)
    }
}
