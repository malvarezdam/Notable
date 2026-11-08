//! `notable-sync`: cuenta y sincronización con Supabase.
//!
//! La app funciona completa sin esto; la sincronización es una capa aparte (ver `PLAN.md`).
//!
//! Cómo se sincroniza ([`sync`]):
//! 1. **Bajar** lo que cambió en el servidor desde la última vez (con un margen, por si el reloj
//!    del servidor ordena algo tarde) y aplicarlo: gana el `updated_at` más reciente.
//! 2. **Subir** lo pendiente (`dirty`). El servidor también se queda con el más reciente (trigger
//!    de `supabase/schema.sql`).
//!
//! Nada se borra de verdad: los borrados son filas con `deleted = true`. Al entrar con una cuenta
//! en un dispositivo que ya tiene datos, todo lo local se marca pendiente y se fusiona.
//!
//! La red está detrás del trait [`Remote`]: [`Supabase`] es el de verdad y las pruebas usan uno
//! en memoria. Los pasos ([`pull_cursor`], [`finish_pull`]…) son públicos para que la app no
//! tenga la base bloqueada mientras espera a la red.

mod protocol;
#[cfg(not(target_arch = "wasm32"))]
mod supabase;
#[cfg(target_arch = "wasm32")]
mod web;

use std::fmt;

use notable_store::{Store, StoreError, SyncRow};
use serde::{Deserialize, Serialize};

pub use notable_store::now_ms;
pub use protocol::SignUp;
#[cfg(not(target_arch = "wasm32"))]
pub use supabase::{Supabase, SupabaseRemote};
#[cfg(target_arch = "wasm32")]
pub use web::WebClient;

/// Proyecto de Supabase de Notable. La URL y la clave *publishable* son públicas: la seguridad
/// la dan las reglas RLS de la tabla (cada usuario solo ve lo suyo).
pub const SUPABASE_URL: &str = "https://augvbftjwvysrozyxvbh.supabase.co";
pub const SUPABASE_KEY: &str = "sb_publishable_FH2J5moWN9POwB1iAxfOvQ_coxAjYU2";

/// Cuánto se retrocede al bajar cambios, por si alguno quedó con una hora de servidor anterior a
/// la última vista (transacciones largas). Bajar dos veces lo mismo no hace daño.
pub const PULL_MARGIN_MS: i64 = 60_000;

const META_SESSION: &str = "sync.session";
const META_CURSOR: &str = "sync.cursor";
const META_USER: &str = "sync.user";
const META_LAST: &str = "sync.last";

/// Sesión iniciada en la cuenta.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    /// Segundos desde 1970 en que vence `access_token`.
    pub expires_at: i64,
    pub user_id: String,
    pub email: String,
}

/// Una fila que llegó del servidor.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteRow {
    pub row: SyncRow,
    /// Milisegundos desde 1970 en que llegó al servidor.
    pub server_updated_at: i64,
}

/// Por qué no se pudo sincronizar.
#[derive(Debug)]
pub enum SyncError {
    /// Sin conexión (no se llegó al servidor).
    Offline(String),
    /// El servidor no responde bien (caído, o el proyecto de Supabase está pausado).
    Unavailable(String),
    /// Hay que volver a iniciar sesión (o el correo/contraseña no sirven).
    Auth(String),
    /// El servidor rechazó la petición.
    Rejected(String),
    /// Error de la base local.
    Store(StoreError),
}

impl SyncError {
    /// `true` si vale la pena reintentar pronto (sin conexión o servidor no disponible).
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Offline(_) | Self::Unavailable(_))
    }
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline(_) => {
                f.write_str("Sin conexión. Los cambios se guardan y se suben al volver.")
            }
            Self::Unavailable(m) => write!(
                f,
                "El servidor no responde ({m}). Si el proyecto de Supabase está pausado, reactívalo en supabase.com."
            ),
            Self::Auth(m) | Self::Rejected(m) => f.write_str(m),
            Self::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<StoreError> for SyncError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

/// El otro lado de la sincronización.
pub trait Remote {
    /// Filas del usuario que llegaron al servidor después de `since` (milisegundos; `None` =
    /// todas).
    fn pull(&mut self, since: Option<i64>) -> Result<Vec<RemoteRow>, SyncError>;
    /// Sube filas (el servidor se queda con la más reciente de cada una).
    fn push(&mut self, rows: &[SyncRow]) -> Result<(), SyncError>;
}

/// Qué se hizo en una sincronización.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Filas que bajaron.
    pub pulled: usize,
    /// De esas, cuántas cambiaron algo aquí.
    pub applied: usize,
    /// Filas que se subieron.
    pub pushed: usize,
}

/// Sincroniza todo de una vez (bajar, aplicar, subir).
pub fn sync(store: &mut Store, remote: &mut impl Remote) -> Result<Report, SyncError> {
    let since = pull_cursor(store)?;
    let rows = remote.pull(since)?;
    let pulled = rows.len();
    let applied = finish_pull(store, &rows)?;
    let pending = store.pending()?;
    remote.push(&pending)?;
    finish_push(store, &pending, now_ms())?;
    Ok(Report {
        pulled,
        applied,
        pushed: pending.len(),
    })
}

/// Desde cuándo bajar cambios (`None` = todo).
pub fn pull_cursor(store: &Store) -> Result<Option<i64>, SyncError> {
    Ok(store
        .meta(META_CURSOR)?
        .and_then(|c| c.parse::<i64>().ok())
        .map(|c| c - PULL_MARGIN_MS))
}

/// Aplica lo que bajó y avanza el cursor. Devuelve cuántas filas cambiaron aquí.
pub fn finish_pull(store: &mut Store, rows: &[RemoteRow]) -> Result<usize, SyncError> {
    let plain: Vec<SyncRow> = rows.iter().map(|r| r.row.clone()).collect();
    let applied = store.apply_remote(&plain)?;
    if let Some(max) = rows.iter().map(|r| r.server_updated_at).max() {
        let current = store.meta(META_CURSOR)?.and_then(|c| c.parse::<i64>().ok());
        if current.is_none_or(|c| max > c) {
            store.set_meta(META_CURSOR, Some(&max.to_string()))?;
        }
    }
    Ok(applied)
}

/// Marca como subido lo que se subió y anota la hora de la última sincronización.
pub fn finish_push(store: &mut Store, pushed: &[SyncRow], now: i64) -> Result<(), SyncError> {
    let versions: Vec<_> = pushed.iter().map(|r| (r.id, r.updated_at)).collect();
    store.mark_synced(&versions)?;
    store.set_meta(META_LAST, Some(&now.to_string()))?;
    Ok(())
}

/// Hora (milisegundos) de la última sincronización completa.
pub fn last_sync(store: &Store) -> Result<Option<i64>, SyncError> {
    Ok(store.meta(META_LAST)?.and_then(|t| t.parse().ok()))
}

/// La sesión guardada, si hay.
pub fn load_session(store: &Store) -> Result<Option<Session>, SyncError> {
    Ok(store
        .meta(META_SESSION)?
        .and_then(|s| serde_json::from_str(&s).ok()))
}

/// Guarda la sesión (o la borra con `None`, al cerrar sesión: los datos locales se quedan).
pub fn save_session(store: &mut Store, session: Option<&Session>) -> Result<(), SyncError> {
    let json = session.map(|s| serde_json::to_string(s).expect("sesión serializable"));
    store.set_meta(META_SESSION, json.as_deref())?;
    Ok(())
}

/// Prepara la base para sincronizar con `user_id` recién iniciado. Si es otra cuenta que la
/// última sincronizada (o la primera vez), todo lo local queda pendiente para fusionarlo con la
/// cuenta y se baja todo desde el principio. Nunca se borra nada.
pub fn bind_account(store: &mut Store, user_id: &str) -> Result<(), SyncError> {
    if store.meta(META_USER)?.as_deref() != Some(user_id) {
        store.mark_all_dirty()?;
        store.set_meta(META_CURSOR, None)?;
        store.set_meta(META_LAST, None)?;
        store.set_meta(META_USER, Some(user_id))?;
    }
    Ok(())
}
