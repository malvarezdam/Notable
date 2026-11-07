//! App de Notable: comandos Tauri que la interfaz llama para leer y modificar el árbol.

mod state;

use std::sync::Mutex;

use notable_api::Screen;
use tauri::{Manager, State};

use state::Data;

/// El estado compartido. Si la base no se pudo abrir, cada comando devuelve ese error (y no se
/// toca nada, para no arriesgar los datos).
struct AppState(Mutex<state::Result<Data>>);

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

#[tauri::command]
fn screen(view: Option<String>, state: State<'_, AppState>) -> state::Result<Screen> {
    with(&state, |d| d.screen(view.as_deref()))
}

#[tauri::command]
fn create(
    parent: Option<String>,
    name: String,
    state: State<'_, AppState>,
) -> state::Result<String> {
    with(&state, |d| d.create(parent.as_deref(), &name))
}

#[tauri::command]
fn rename(id: String, name: String, state: State<'_, AppState>) -> state::Result<()> {
    with(&state, |d| d.rename(&id, &name))
}

#[tauri::command]
fn delete(id: String, state: State<'_, AppState>) -> state::Result<()> {
    with(&state, |d| d.delete(&id))
}

#[tauri::command]
fn restore(id: String, state: State<'_, AppState>) -> state::Result<()> {
    with(&state, |d| d.restore(&id))
}

#[tauri::command]
fn set_grade(id: String, grade: Option<f64>, state: State<'_, AppState>) -> state::Result<()> {
    with(&state, |d| d.set_grade(&id, grade))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let data = Data::open(&dir.join("notable.db"));
            app.manage(AppState(Mutex::new(data)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            screen, create, rename, delete, restore, set_grade
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Notable");
}
