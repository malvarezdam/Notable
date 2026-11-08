//! App de Notable: comandos Tauri que la interfaz llama para leer y modificar el árbol, y la
//! cuenta con sincronización (opcional).

mod state;
mod sync;

use std::sync::Mutex;

use notable_api::{Account, Change, Destination, Screen, SyncStatus};
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
fn destinations(id: String, state: State<'_, AppState>) -> state::Result<Vec<Destination>> {
    with(&state, |d| d.destinations(&id))
}

#[tauri::command]
fn move_to(
    id: String,
    parent: Option<String>,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<()> {
    change(&state, &sync, |d| d.move_to(&id, parent.as_deref()))
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

/// Cambia la contraseña de la cuenta. Devuelve un mensaje para mostrar.
#[tauri::command]
async fn change_password(
    current: String,
    password: String,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<String> {
    let session = with(&state, |d| d.session())?.ok_or("Primero inicia sesión.")?;
    let fresh = blocking(move || {
        Supabase::default()
            .change_password(&session.email, &current, &password)
            .map_err(|e| e.to_string())
    })
    .await?;
    with(&state, |d| d.store_session(Some(&fresh)))?;
    sync.now();
    Ok(
        "Contraseña cambiada. En tus otros dispositivos puede que debas entrar con la nueva."
            .into(),
    )
}

#[tauri::command]
fn sync_now(sync: State<'_, SyncHandle>) {
    sync.now();
}

// ------------------------------------------------------------------ respaldo

/// Guarda un respaldo en JSON donde elija el usuario. Devuelve un mensaje, o `None` si canceló.
#[tauri::command]
async fn export_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> state::Result<Option<String>> {
    use std::io::Write;
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_fs::{FsExt, OpenOptions};

    let json = with(&state, |d| d.export_backup())?;
    let name = format!("notable-respaldo-{}.json", today());
    let handle = app.clone();
    let path = blocking(move || {
        Ok(handle
            .dialog()
            .file()
            .add_filter("Respaldo de Notable", &["json"])
            .set_file_name(name)
            .blocking_save_file())
    })
    .await?;
    let Some(path) = path else {
        return Ok(None);
    };
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = app
        .fs()
        .open(path, options)
        .map_err(|e| format!("no se pudo crear el archivo: {e}"))?;
    file.write_all(json.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|e| format!("no se pudo escribir el respaldo: {e}"))?;
    Ok(Some("Respaldo guardado.".into()))
}

/// Importa un respaldo elegido por el usuario, sin borrar nada. Devuelve un mensaje, o `None` si
/// canceló.
#[tauri::command]
async fn import_backup(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    sync: State<'_, SyncHandle>,
) -> state::Result<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    use tauri_plugin_fs::FsExt;

    let handle = app.clone();
    let path = blocking(move || {
        Ok(handle
            .dialog()
            .file()
            .add_filter("Respaldo de Notable", &["json"])
            .blocking_pick_file())
    })
    .await?;
    let Some(path) = path else {
        return Ok(None);
    };
    let json = app
        .fs()
        .read_to_string(path)
        .map_err(|e| format!("no se pudo leer el archivo: {e}"))?;
    let count = change(&state, &sync, |d| d.import_backup(&json))?;
    Ok(Some(format!(
        "Respaldo importado ({count} elementos). No se borró nada de lo que ya tenías."
    )))
}

/// Fecha de hoy (UTC) como "2026-11-07", para el nombre del respaldo.
fn today() -> String {
    civil_date(notable_sync::now_ms().div_euclid(86_400_000))
}

/// Días desde 1970 → "AAAA-MM-DD" (algoritmo de Howard Hinnant).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
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
            screen,
            create,
            rename,
            delete,
            restore,
            set_grade,
            shift,
            destinations,
            move_to,
            update,
            account,
            sign_in,
            sign_up,
            sign_out,
            change_password,
            sync_now,
            export_backup,
            import_backup
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Notable");
}

#[cfg(test)]
mod tests {
    use super::civil_date;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(11_016), "2000-02-29");
        assert_eq!(civil_date(20_764), "2026-11-07");
    }
}
