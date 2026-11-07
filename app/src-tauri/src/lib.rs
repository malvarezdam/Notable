//! App de Notable: comandos Tauri que la interfaz llama para leer y modificar el árbol, y la
//! cuenta con sincronización (opcional).

mod state;
mod sync;

use std::sync::Mutex;

use notable_api::{Account, Change, Screen, SyncStatus};
use notable_sync::{SignUp, Supabase};
use tauri::{Manager, State};

use state::Data;
use sync::SyncHandle;

/// El estado compartido. Si la base no se pudo abrir, cada comando devuelve ese error (y no se
/// toca nada, para no arriesgar los datos).
pub(crate) struct AppState(pub(crate) Mutex<state::Result<Data>>);

fn with<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Data) -> state::Result<T>,
) -> state::Result<T> {
    let mut guard = state.0.lock().map_err(|_| "estado bloqueado".to_owned())?;
    match guard.as_mut() {
        Ok(data) => f(data),
        Err(e) => Err(format!("no se pudo abrir la base de datos: {e}")),
    }
}

/// Como [`with`], y si sale bien pide sincronizar en unos segundos.
fn change<T>(
    state: &State<'_, AppState>,
    sync: &State<'_, SyncHandle>,
    f: impl FnOnce(&mut Data) -> state::Result<T>,
) -> state::Result<T> {
    let result = with(state, f);
    if result.is_ok() {
        sync.soon();
    }
    result
}

#[tauri::command]
fn screen(view: Option<String>, state: State<'_, AppState>) -> state::Result<Screen> {
    with(&state, |d| d.screen(view.as_deref()))
}

#[tauri::command]
fn create(
    parent: Option<String>,
    name: String,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<String> {
    change(&state, &sync, |d| d.create(parent.as_deref(), &name))
}

#[tauri::command]
fn rename(
    id: String,
    name: String,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<()> {
    change(&state, &sync, |d| d.rename(&id, &name))
}

#[tauri::command]
fn delete(
    id: String,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<()> {
    change(&state, &sync, |d| d.delete(&id))
}

#[tauri::command]
fn restore(
    id: String,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<()> {
    change(&state, &sync, |d| d.restore(&id))
}

#[tauri::command]
fn set_grade(
    id: String,
    grade: Option<f64>,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<()> {
    change(&state, &sync, |d| d.set_grade(&id, grade))
}

#[tauri::command]
fn shift(
    id: String,
    delta: i32,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<()> {
    change(&state, &sync, |d| d.shift(&id, delta))
}

#[tauri::command]
fn update(
    id: String,
    change: Change,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<()> {
    self::change(&state, &sync, |d| d.update(&id, change))
}

// ------------------------------------------------------------------ cuenta

/// Estado de la cuenta para el indicador y el panel.
#[tauri::command]
fn account(state: State<'_, AppState>, sync: State<'_, SyncHandle>) -> state::Result<Account> {
    let info = sync.info();
    with(&state, |d| {
        let session = d.session()?;
        let pending = d.pending_count();
        let status = match (&session, info.status) {
            (None, _) => SyncStatus::SignedOut,
            // Recién entró y el hilo todavía no corre.
            (Some(_), SyncStatus::SignedOut) => SyncStatus::Syncing,
            (Some(_), s) => s,
        };
        let message = match status {
            SyncStatus::SignedOut if !info.error.is_empty() => info.error.clone(),
            SyncStatus::SignedOut => {
                "Sin cuenta: tus notas se guardan solo en este dispositivo.".into()
            }
            SyncStatus::Syncing => "Sincronizando…".into(),
            SyncStatus::Synced => match d.last_sync() {
                Some(t) => format!("Sincronizado {}", sync::ago(t)),
                None => "Sincronizado".into(),
            },
            SyncStatus::Offline | SyncStatus::Error => info.error.clone(),
        };
        Ok(Account {
            email: session.map(|s| s.email),
            status,
            message,
            pending,
            revision: info.revision,
        })
    })
}

/// Corre algo bloqueante (red) fuera del hilo de la interfaz.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn sign_in(
    email: String,
    password: String,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<()> {
    let session = blocking(move || {
        Supabase::default()
            .sign_in(&email, &password)
            .map_err(|e| e.to_string())
    })
    .await?;
    with(&state, |d| d.signed_in(&session))?;
    sync.now();
    Ok(())
}

/// Crea una cuenta. Devuelve un mensaje para mostrar.
#[tauri::command]
async fn sign_up(
    email: String,
    password: String,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<String> {
    let result = blocking(move || {
        Supabase::default()
            .sign_up(&email, &password)
            .map_err(|e| e.to_string())
    })
    .await?;
    match result {
        SignUp::Session(session) => {
            with(&state, |d| d.signed_in(&session))?;
            sync.now();
            Ok("Cuenta creada. Ya estás sincronizando.".into())
        }
        SignUp::ConfirmEmail => Ok(
            "Te enviamos un correo para confirmar la cuenta. Ábrelo y después pulsa «Entrar»."
                .into(),
        ),
    }
}

/// Cierra la sesión. Los datos de este dispositivo se quedan.
#[tauri::command]
async fn sign_out(state: State<'_, AppState>, sync: State<'_, SyncHandle>) -> state::Result<()> {
    let session = with(&state, |d| d.session())?;
    with(&state, |d| d.store_session(None))?;
    sync.now();
    if let Some(session) = session {
        // Avisar al servidor es opcional: si no hay conexión, la sesión igual queda olvidada.
        let _ = blocking(move || {
            Supabase::default().sign_out(&session);
            Ok(())
        })
        .await;
    }
    Ok(())
}

#[tauri::command]
fn sync_now(sync: State<'_, SyncHandle>) {
    sync.now();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let data = Data::open(&dir.join("notable.db"));
            app.manage(AppState(Mutex::new(data)));
            let (handle, wake) = sync::new();
            app.manage(handle);
            sync::spawn(app.handle().clone(), wake);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            screen, create, rename, delete, restore, set_grade, shift, update, account, sign_in,
            sign_up, sign_out, sync_now
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Notable");
}
