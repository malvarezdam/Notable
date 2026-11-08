//! Cliente de Supabase para el navegador: lo mismo que `supabase.rs`, con `fetch`.

use notable_store::SyncRow;
use serde_json::{Value, json};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestCache, RequestInit, Response};

use crate::protocol::*;
use crate::{RemoteRow, SUPABASE_KEY, SUPABASE_URL, Session, SyncError, now_ms};

/// Filas por página al bajar y por petición al subir.
const PAGE: usize = 500;

/// Margen para refrescar la sesión antes de que venza.
const REFRESH_MARGIN_S: i64 = 120;

const OFFLINE: &str = "sin conexión";

/// Cliente de Supabase en el navegador.
#[derive(Clone, Copy, Default)]
pub struct WebClient;

fn offline(_: JsValue) -> SyncError {
    SyncError::Offline(OFFLINE.to_owned())
}

/// Hace una petición y devuelve el estado y el cuerpo.
async fn send(
    method: &str,
    url: &str,
    token: Option<&str>,
    prefer: Option<&str>,
    body: Option<&Value>,
) -> Result<(u16, String), SyncError> {
    let init = RequestInit::new();
    init.set_method(method);
    init.set_cache(RequestCache::NoStore);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(&body.to_string()));
    }
    let request = Request::new_with_str_and_init(url, &init).map_err(offline)?;
    let headers = request.headers();
    let set = |name: &str, value: &str| headers.set(name, value).map_err(offline);
    set("apikey", SUPABASE_KEY)?;
    if body.is_some() {
        set("Content-Type", "application/json")?;
    }
    if let Some(token) = token {
        set("Authorization", &format!("Bearer {token}"))?;
    }
    if let Some(prefer) = prefer {
        set("Prefer", prefer)?;
    }
    let window = web_sys::window().ok_or_else(|| offline(JsValue::NULL))?;
    let response: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(offline)?
        .unchecked_into();
    let text = JsFuture::from(response.text().map_err(offline)?)
        .await
        .map_err(offline)?
        .as_string()
        .unwrap_or_default();
    Ok((response.status(), text))
}

/// El cuerpo si la respuesta fue exitosa; si no, el error para mostrar.
fn checked(status: u16, text: &str, auth: bool) -> Result<Value, SyncError> {
    let body: Value = if text.trim().is_empty() {
        Value::Null
    } else {
        match serde_json::from_str(text) {
            Ok(value) => value,
            Err(e) if (200..300).contains(&status) => {
                return Err(SyncError::Rejected(format!("respuesta inválida: {e}")));
            }
            Err(_) => Value::Null,
        }
    };
    if (200..300).contains(&status) {
        Ok(body)
    } else {
        Err(http_error(status, &body, auth))
    }
}

fn encode(text: &str) -> String {
    String::from(js_sys::encode_uri_component(text))
}

impl WebClient {
    async fn auth_post(self, path: &str, body: &Value) -> Result<Value, SyncError> {
        let url = format!("{SUPABASE_URL}/auth/v1/{path}");
        let (status, text) = send("POST", &url, None, None, Some(body)).await?;
        checked(status, &text, true)
    }

    /// Inicia sesión con correo y contraseña.
    pub async fn sign_in(self, email: &str, password: &str) -> Result<Session, SyncError> {
        let body = json!({ "email": email.trim(), "password": password });
        session_from(self.auth_post("token?grant_type=password", &body).await?)
    }

    /// Crea una cuenta.
    pub async fn sign_up(self, email: &str, password: &str) -> Result<SignUp, SyncError> {
        let body = json!({ "email": email.trim(), "password": password });
        let value = self.auth_post("signup", &body).await?;
        if value.get("access_token").is_some() {
            session_from(value).map(SignUp::Session)
        } else {
            Ok(SignUp::ConfirmEmail)
        }
    }

    /// Renueva la sesión con el `refresh_token`.
    pub async fn refresh(self, session: &Session) -> Result<Session, SyncError> {
        let body = json!({ "refresh_token": session.refresh_token });
        match self
            .auth_post("token?grant_type=refresh_token", &body)
            .await
        {
            Ok(value) => session_from(value),
            // Un refresh rechazado significa que hay que volver a entrar.
            Err(SyncError::Rejected(m)) => Err(SyncError::Auth(m)),
            Err(e) => Err(e),
        }
    }

    /// Cierra la sesión en el servidor (si falla, da igual: localmente se olvida igual).
    pub async fn sign_out(self, session: &Session) {
        let url = format!("{SUPABASE_URL}/auth/v1/logout");
        let _ = send("POST", &url, Some(&session.access_token), None, None).await;
    }

    async fn set_password(self, session: &Session, new: &str) -> Result<(), SyncError> {
        let url = format!("{SUPABASE_URL}/auth/v1/user");
        let body = json!({ "password": new });
        let (status, text) =
            send("PUT", &url, Some(&session.access_token), None, Some(&body)).await?;
        checked(status, &text, true).map(|_| ())
    }

    /// Cambia la contraseña comprobando antes la actual. Devuelve la sesión que hay que guardar.
    pub async fn change_password(
        self,
        email: &str,
        current: &str,
        new: &str,
    ) -> Result<Session, SyncError> {
        let session = self.sign_in(email, current).await.map_err(|e| match e {
            SyncError::Rejected(m) | SyncError::Auth(m) if m == WRONG_CREDENTIALS => {
                SyncError::Rejected("La contraseña actual no es correcta.".to_owned())
            }
            e => e,
        })?;
        self.set_password(&session, new).await?;
        Ok(session)
    }

    /// Pide el código para recuperar la contraseña.
    pub async fn send_recovery(self, email: &str) -> Result<(), SyncError> {
        self.auth_post("recover", &json!({ "email": email.trim() }))
            .await?;
        Ok(())
    }

    /// Cambia una contraseña olvidada con el código del correo y devuelve la sesión iniciada.
    pub async fn reset_password(
        self,
        email: &str,
        code: &str,
        new: &str,
    ) -> Result<Session, SyncError> {
        if new.chars().count() < MIN_PASSWORD {
            return Err(SyncError::Rejected(WEAK_PASSWORD.to_owned()));
        }
        let body = json!({ "type": "recovery", "email": email.trim(), "token": code.trim() });
        let session = session_from(self.auth_post("verify", &body).await?)?;
        self.set_password(&session, new)
            .await
            .map_err(|e| match e {
                SyncError::Rejected(m) | SyncError::Auth(m) => SyncError::Rejected(format!(
                    "{m} Pide un código nuevo para intentarlo otra vez."
                )),
                e => e,
            })?;
        Ok(session)
    }

    /// Petición a los datos: renueva la sesión si hace falta y reintenta una vez si venció.
    async fn data(
        self,
        session: &mut Session,
        method: &str,
        url: &str,
        prefer: Option<&str>,
        body: Option<&Value>,
    ) -> Result<Value, SyncError> {
        if session.expires_at - REFRESH_MARGIN_S <= now_ms() / 1000 {
            *session = self.refresh(session).await?;
        }
        let mut reply = send(method, url, Some(&session.access_token), prefer, body).await?;
        if reply.0 == 401 {
            *session = self.refresh(session).await?;
            reply = send(method, url, Some(&session.access_token), prefer, body).await?;
        }
        checked(reply.0, &reply.1, false)
    }

    /// Filas que llegaron al servidor después de `since`.
    pub async fn pull(
        self,
        session: &mut Session,
        since: Option<i64>,
    ) -> Result<Vec<RemoteRow>, SyncError> {
        let filter = match since {
            Some(ms) => format!("&server_updated_at=gt.{}", encode(&format_ms(ms)?)),
            None => String::new(),
        };
        let mut rows = Vec::new();
        // Paginar por posición: un lote subido de una vez comparte la misma hora de servidor.
        loop {
            let url = format!(
                "{SUPABASE_URL}/rest/v1/items?select=id,parent_id,data,updated_at,deleted,server_updated_at\
                 &order=server_updated_at.asc,id.asc&limit={PAGE}&offset={}{filter}",
                rows.len()
            );
            let value = self.data(session, "GET", &url, None, None).await?;
            let page: Vec<ItemRow> = serde_json::from_value(value)
                .map_err(|e| SyncError::Rejected(format!("respuesta inválida: {e}")))?;
            let count = page.len();
            for item in page {
                rows.push(to_remote(item)?);
            }
            if count < PAGE {
                return Ok(rows);
            }
        }
    }

    /// Sube filas (el servidor se queda con la más reciente de cada una).
    pub async fn push(self, session: &mut Session, rows: &[SyncRow]) -> Result<(), SyncError> {
        let url = format!("{SUPABASE_URL}/rest/v1/items");
        for chunk in rows.chunks(PAGE) {
            let body = Value::Array(chunk.iter().map(row_json).collect::<Result<Vec<_>, _>>()?);
            self.data(
                session,
                "POST",
                &url,
                Some("resolution=merge-duplicates,return=minimal"),
                Some(&body),
            )
            .await?;
        }
        Ok(())
    }
}
