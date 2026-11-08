//! Versión web: hace dentro del navegador lo que en la app nativa hacen los comandos de Tauri.
//! El estado y las operaciones son los mismos (`notable-state`); solo cambian el guardado, las
//! peticiones a Supabase y los archivos de respaldo.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use leptos::prelude::{on_cleanup, set_interval, set_timeout, window_event_listener};
use leptos::task::spawn_local;
use notable_api::{Change, SyncStatus};
use notable_state::{Data, today};
use notable_store::STORAGE_KEY;
use notable_sync::{SignUp, SyncError, WebClient, now_ms};
use serde::Serialize;
use serde_json::Value;
use wasm_bindgen::prelude::*;

/// Cada cuánto se sincroniza si no pasa nada.
const EVERY_MS: i64 = 180_000;
/// Cada cuánto se reintenta sin conexión.
const RETRY_OFFLINE_MS: i64 = 30_000;
/// Cuánto esperar tras un cambio (para juntar varios cambios seguidos en una subida).
const AFTER_CHANGE: Duration = Duration::from_secs(4);

#[wasm_bindgen(inline_js = r#"
export function downloadText(name, text) {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}

// Devuelve el texto del archivo elegido, o null si se canceló.
export function pickTextFile() {
  return new Promise((resolve, reject) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,application/json";
    let picked = false;
    input.addEventListener("change", () => {
      picked = true;
      const file = input.files[0];
      if (file) file.text().then(resolve, reject); else resolve(null);
    });
    input.addEventListener("cancel", () => resolve(null));
    // Navegadores sin el evento `cancel`: al volver sin elegir, se da por cancelado.
    window.addEventListener("focus", () => {
      setTimeout(() => { if (!picked) resolve(null); }, 1000);
    }, { once: true });
    input.click();
  });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = downloadText)]
    fn download_text(name: &str, text: &str);
    #[wasm_bindgen(js_name = pickTextFile, catch)]
    async fn pick_text_file() -> Result<JsValue, JsValue>;
}

struct SyncState {
    status: SyncStatus,
    error: String,
    revision: u64,
    last_run: i64,
}

thread_local! {
    static DATA: RefCell<Result<Data, String>> =
        RefCell::new(Err("la app todavía no abre sus datos".to_owned()));
    static SYNC: RefCell<SyncState> = const {
        RefCell::new(SyncState {
            status: SyncStatus::SignedOut,
            error: String::new(),
            revision: 0,
            last_run: 0,
        })
    };
    static RUNNING: Cell<bool> = const { Cell::new(false) };
    static AGAIN: Cell<bool> = const { Cell::new(false) };
    /// Sube con cada cambio: solo el último "sincronizar en unos segundos" corre.
    static SOON: Cell<u64> = const { Cell::new(0) };
}

fn with<T>(f: impl FnOnce(&mut Data) -> Result<T, String>) -> Result<T, String> {
    DATA.with_borrow_mut(|data| match data {
        Ok(data) => f(data),
        Err(e) => Err(format!("no se pudieron abrir los datos: {e}")),
    })
}

fn set_sync(status: SyncStatus, error: String, changed: bool) {
    SYNC.with_borrow_mut(|s| {
        s.status = status;
        s.error = error;
        if changed {
            s.revision += 1;
        }
    });
}

/// Abre los datos y deja corriendo la sincronización.
pub fn start() {
    DATA.set(Data::open_browser());
    sync_now();
    set_interval(
        || {
            let (status, last) = SYNC.with_borrow(|s| (s.status, s.last_run));
            let wait = if status == SyncStatus::Offline {
                RETRY_OFFLINE_MS
            } else {
                EVERY_MS
            };
            if now_ms() - last >= wait {
                sync_now();
            }
        },
        Duration::from_secs(10),
    );
    // Otra pestaña cambió los datos: mostrar lo nuevo.
    let listener = window_event_listener(leptos::ev::storage, |ev| {
        if ev.key().is_none_or(|key| key == STORAGE_KEY) && with(|d| d.reload()).is_ok() {
            SYNC.with_borrow_mut(|s| s.revision += 1);
        }
    });
    on_cleanup(move || listener.remove());
}

fn sync_now() {
    if RUNNING.get() {
        AGAIN.set(true);
        return;
    }
    RUNNING.set(true);
    spawn_local(async {
        loop {
            AGAIN.set(false);
            run_once().await;
            if !AGAIN.get() {
                break;
            }
        }
        RUNNING.set(false);
    });
}

fn sync_soon() {
    let ticket = SOON.get() + 1;
    SOON.set(ticket);
    set_timeout(
        move || {
            if SOON.get() == ticket {
                sync_now();
            }
        },
        AFTER_CHANGE,
    );
}

/// Una sincronización completa: la misma secuencia que la app nativa.
async fn run_once() {
    SYNC.with_borrow_mut(|s| s.last_run = now_ms());
    let (mut session, since) = match with(|d| d.sync_begin()) {
        Ok(Some(begin)) => begin,
        Ok(None) => {
            set_sync(SyncStatus::SignedOut, String::new(), false);
            return;
        }
        Err(e) => {
            set_sync(SyncStatus::Error, e, false);
            return;
        }
    };
    let original = session.clone();
    set_sync(SyncStatus::Syncing, String::new(), false);

    let client = WebClient;
    let result: Result<usize, SyncError> = async {
        let rows = client.pull(&mut session, since).await?;
        let (pending, applied) = with(|d| d.sync_apply(&rows)).map_err(SyncError::Rejected)?;
        client.push(&mut session, &pending).await?;
        with(|d| d.sync_end(&pending, now_ms())).map_err(SyncError::Rejected)?;
        Ok(applied)
    }
    .await;

    match result {
        Ok(applied) => {
            if session != original {
                let _ = with(|d| d.store_session_if(&original, Some(&session)));
            }
            set_sync(SyncStatus::Synced, String::new(), applied > 0);
        }
        Err(SyncError::Auth(message)) => {
            // La sesión ya no sirve: se olvida (los datos locales se quedan).
            let _ = with(|d| d.store_session_if(&original, None));
            set_sync(SyncStatus::SignedOut, message, false);
        }
        Err(e) => {
            if session != original {
                let _ = with(|d| d.store_session_if(&original, Some(&session)));
            }
            let status = if e.is_transient() {
                SyncStatus::Offline
            } else {
                SyncStatus::Error
            };
            set_sync(status, e.to_string(), false);
        }
    }
}

fn text(args: &Value, name: &str) -> Result<String, String> {
    args[name]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("falta {name}"))
}

fn to_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

fn message(e: SyncError) -> String {
    e.to_string()
}

/// Un cambio en los datos: si sale bien, sincroniza en unos segundos.
fn change<T: Serialize>(f: impl FnOnce(&mut Data) -> Result<T, String>) -> Result<Value, String> {
    let value = with(f)?;
    sync_soon();
    to_value(value)
}

/// Ejecuta un comando de la interfaz.
pub async fn dispatch(command: &str, args: Value) -> Result<Value, String> {
    let client = WebClient;
    match command {
        "screen" => to_value(with(|d| d.screen(args["view"].as_str()))?),
        "create" => {
            let name = text(&args, "name")?;
            change(|d| d.create(args["parent"].as_str(), &name))
        }
        "rename" => {
            let (id, name) = (text(&args, "id")?, text(&args, "name")?);
            change(|d| d.rename(&id, &name))
        }
        "delete" => {
            let id = text(&args, "id")?;
            change(|d| d.delete(&id))
        }
        "restore" => {
            let id = text(&args, "id")?;
            change(|d| d.restore(&id))
        }
        "set_grade" => {
            let id = text(&args, "id")?;
            change(|d| d.set_grade(&id, args["grade"].as_f64()))
        }
        "shift" => {
            let id = text(&args, "id")?;
            let delta = args["delta"].as_i64().unwrap_or(0) as i32;
            change(|d| d.shift(&id, delta))
        }
        "destinations" => {
            let id = text(&args, "id")?;
            to_value(with(|d| d.destinations(&id))?)
        }
        "move_to" => {
            let id = text(&args, "id")?;
            change(|d| d.move_to(&id, args["parent"].as_str()))
        }
        "update" => {
            let id = text(&args, "id")?;
            let update: Change =
                serde_json::from_value(args["change"].clone()).map_err(|e| e.to_string())?;
            change(|d| d.update(&id, update))
        }
        "account" => {
            let (status, error, revision) =
                SYNC.with_borrow(|s| (s.status, s.error.clone(), s.revision));
            to_value(with(|d| d.account(status, &error, revision))?)
        }
        "sign_in" => {
            let session = client
                .sign_in(&text(&args, "email")?, &text(&args, "password")?)
                .await
                .map_err(message)?;
            with(|d| d.signed_in(&session))?;
            sync_now();
            Ok(Value::Null)
        }
        "sign_up" => {
            let result = client
                .sign_up(&text(&args, "email")?, &text(&args, "password")?)
                .await
                .map_err(message)?;
            to_value(match result {
                SignUp::Session(session) => {
                    with(|d| d.signed_in(&session))?;
                    sync_now();
                    "Cuenta creada. Ya estás sincronizando."
                }
                SignUp::ConfirmEmail => {
                    "Te enviamos un correo para confirmar la cuenta. Ábrelo y después pulsa «Entrar»."
                }
            })
        }
        "sign_out" => {
            let session = with(|d| d.session())?;
            with(|d| d.store_session(None))?;
            sync_now();
            if let Some(session) = session {
                client.sign_out(&session).await;
            }
            Ok(Value::Null)
        }
        "change_password" => {
            let session = with(|d| d.session())?.ok_or("Primero inicia sesión.")?;
            let fresh = client
                .change_password(
                    &session.email,
                    &text(&args, "current")?,
                    &text(&args, "password")?,
                )
                .await
                .map_err(message)?;
            with(|d| d.store_session(Some(&fresh)))?;
            sync_now();
            to_value(
                "Contraseña cambiada. En tus otros dispositivos puede que debas entrar con la nueva.",
            )
        }
        "send_recovery" => {
            client
                .send_recovery(&text(&args, "email")?)
                .await
                .map_err(message)?;
            to_value(
                "Si hay una cuenta con ese correo, te enviamos un código. Revisa también el spam.",
            )
        }
        "reset_password" => {
            let session = client
                .reset_password(
                    &text(&args, "email")?,
                    &text(&args, "code")?,
                    &text(&args, "password")?,
                )
                .await
                .map_err(message)?;
            with(|d| d.signed_in(&session))?;
            sync_now();
            to_value("Contraseña cambiada. Ya iniciaste sesión.")
        }
        "sync_now" => {
            sync_now();
            Ok(Value::Null)
        }
        "export_backup" => {
            let json = with(|d| d.export_backup())?;
            download_text(&format!("notable-respaldo-{}.json", today()), &json);
            to_value(Some("Respaldo guardado en tus descargas."))
        }
        "import_backup" => {
            let picked = pick_text_file()
                .await
                .map_err(|_| "no se pudo leer el archivo".to_owned())?;
            let Some(json) = picked.as_string() else {
                return Ok(Value::Null);
            };
            let count = with(|d| d.import_backup(&json))?;
            sync_soon();
            to_value(Some(format!(
                "Respaldo importado ({count} elementos). No se borró nada de lo que ya tenías."
            )))
        }
        other => Err(format!("comando desconocido: {other}")),
    }
}
