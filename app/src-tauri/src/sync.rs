//! Sincronización en segundo plano: un hilo que sincroniza al abrir, cada pocos minutos, poco
//! después de cada cambio, cuando se pide ("Sincronizar ahora" o al volver la conexión) y más
//! seguido mientras no hay conexión, para subir apenas vuelva.

use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use notable_api::SyncStatus;
use notable_sync::{Remote, Supabase, SyncError, now_ms};
use tauri::{AppHandle, Manager};

use crate::AppState;
use notable_state::Data;

/// Cada cuánto se sincroniza si no pasa nada.
const EVERY: Duration = Duration::from_secs(180);
/// Cada cuánto se reintenta sin conexión.
const RETRY_OFFLINE: Duration = Duration::from_secs(30);
/// Cuánto esperar tras un cambio (para juntar varios cambios seguidos en una subida).
const AFTER_CHANGE: Duration = Duration::from_secs(4);

pub enum Wake {
    Now,
    Soon,
}

/// Estado visible de la sincronización.
#[derive(Clone)]
pub struct Info {
    pub status: SyncStatus,
    /// Mensaje del último error (vacío si salió bien).
    pub error: String,
    pub revision: u64,
}

/// Lo que la app comparte con el hilo de sincronización.
pub struct SyncHandle {
    wake: Mutex<Sender<Wake>>,
    pub info: Mutex<Info>,
}

impl SyncHandle {
    /// Sincronizar ya.
    pub fn now(&self) {
        if let Ok(tx) = self.wake.lock() {
            let _ = tx.send(Wake::Now);
        }
    }

    /// Sincronizar en unos segundos (después de un cambio).
    pub fn soon(&self) {
        if let Ok(tx) = self.wake.lock() {
            let _ = tx.send(Wake::Soon);
        }
    }

    pub fn info(&self) -> Info {
        self.info.lock().map(|i| i.clone()).unwrap_or(Info {
            status: SyncStatus::Error,
            error: "estado bloqueado".into(),
            revision: 0,
        })
    }

    fn set(&self, status: SyncStatus, error: String, changed: bool) {
        if let Ok(mut info) = self.info.lock() {
            info.status = status;
            info.error = error;
            if changed {
                info.revision += 1;
            }
        }
    }
}

/// Crea el estado compartido. Registrarlo con `manage` y después llamar a [`spawn`].
pub fn new() -> (SyncHandle, Receiver<Wake>) {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(Wake::Now); // sincronizar al abrir
    let handle = SyncHandle {
        wake: Mutex::new(tx),
        info: Mutex::new(Info {
            status: SyncStatus::SignedOut,
            error: String::new(),
            revision: 0,
        }),
    };
    (handle, rx)
}

/// Lanza el hilo de sincronización (con `AppState` y `SyncHandle` ya registrados).
pub fn spawn(app: AppHandle, rx: Receiver<Wake>) {
    std::thread::spawn(move || {
        let mut offline = false;
        loop {
            let wait = if offline { RETRY_OFFLINE } else { EVERY };
            match rx.recv_timeout(wait) {
                Ok(Wake::Soon) => {
                    std::thread::sleep(AFTER_CHANGE);
                    while rx.try_recv().is_ok() {}
                }
                Ok(Wake::Now) => while rx.try_recv().is_ok() {},
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            offline = run_once(&app);
        }
    });
}

/// Usa los datos con el candado tomado.
fn with_data<T>(
    app: &AppHandle,
    f: impl FnOnce(&mut Data) -> Result<T, String>,
) -> Result<T, String> {
    let state = app.state::<AppState>();
    let mut guard = state.0.lock().map_err(|_| "estado bloqueado".to_owned())?;
    match guard.as_mut() {
        Ok(data) => f(data),
        Err(e) => Err(format!("no se pudo abrir la base de datos: {e}")),
    }
}

/// Una sincronización completa. Devuelve `true` si quedó sin conexión (reintentar pronto).
fn run_once(app: &AppHandle) -> bool {
    let Some(sync) = app.try_state::<SyncHandle>() else {
        return false;
    };
    let begin = match with_data(app, |d| d.sync_begin()) {
        Ok(Some(begin)) => begin,
        Ok(None) => {
            sync.set(SyncStatus::SignedOut, String::new(), false);
            return false;
        }
        Err(e) => {
            sync.set(SyncStatus::Error, e, false);
            return false;
        }
    };
    let (mut session, since) = begin;
    let original = session.clone();
    sync.set(SyncStatus::Syncing, String::new(), false);

    let client = Supabase::default();
    let result = (|| -> Result<usize, SyncError> {
        let mut remote = client.remote(&mut session);
        let rows = remote.pull(since)?;
        let (pending, applied) =
            with_data(app, |d| d.sync_apply(&rows)).map_err(SyncError::Rejected)?;
        remote.push(&pending)?;
        with_data(app, |d| d.sync_end(&pending, now_ms())).map_err(SyncError::Rejected)?;
        Ok(applied)
    })();

    match result {
        Ok(applied) => {
            if session != original {
                let _ = with_data(app, |d| d.store_session_if(&original, Some(&session)));
            }
            sync.set(SyncStatus::Synced, String::new(), applied > 0);
            false
        }
        Err(SyncError::Auth(message)) => {
            // La sesión ya no sirve: se olvida (los datos locales se quedan).
            let _ = with_data(app, |d| d.store_session_if(&original, None));
            sync.set(SyncStatus::SignedOut, message, false);
            false
        }
        Err(e) => {
            if session != original {
                let _ = with_data(app, |d| d.store_session_if(&original, Some(&session)));
            }
            let transient = e.is_transient();
            let status = if transient {
                SyncStatus::Offline
            } else {
                SyncStatus::Error
            };
            sync.set(status, e.to_string(), false);
            transient
        }
    }
}
