//! Cliente de Supabase por HTTP: Auth (correo y contraseña) y la tabla `items` (PostgREST).

use std::time::Duration;

use notable_core::NodeId;
use notable_store::SyncRow;
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{Remote, RemoteRow, SUPABASE_KEY, SUPABASE_URL, Session, SyncError};

/// Filas por página al bajar y por petición al subir.
const PAGE: usize = 500;

/// Margen para refrescar la sesión antes de que venza.
const REFRESH_MARGIN_S: i64 = 120;

/// Resultado de crear una cuenta.
#[derive(Debug, PartialEq)]
pub enum SignUp {
    /// La cuenta quedó lista y con sesión iniciada.
    Session(Session),
    /// Hay que confirmar el correo antes de entrar.
    ConfirmEmail,
}

/// Cliente de Supabase.
pub struct Supabase {
    url: String,
    key: String,
    agent: ureq::Agent,
}

impl Default for Supabase {
    fn default() -> Self {
        Self::new(SUPABASE_URL, SUPABASE_KEY)
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
    user: User,
}

#[derive(Deserialize)]
struct User {
    id: String,
    #[serde(default)]
    email: Option<String>,
}

impl Supabase {
    pub fn new(url: &str, key: &str) -> Self {
        Self {
            url: url.trim_end_matches('/').to_owned(),
            key: key.to_owned(),
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build(),
        }
    }

    /// Inicia sesión con correo y contraseña.
    pub fn sign_in(&self, email: &str, password: &str) -> Result<Session, SyncError> {
        let body = json!({ "email": email.trim(), "password": password });
        let value = self.auth_post("token?grant_type=password", &body)?;
        session_from(value)
    }

    /// Crea una cuenta. Según la configuración del proyecto, queda con sesión o pide confirmar
    /// el correo.
    pub fn sign_up(&self, email: &str, password: &str) -> Result<SignUp, SyncError> {
        let body = json!({ "email": email.trim(), "password": password });
        let value = self.auth_post("signup", &body)?;
        if value.get("access_token").is_some() {
            session_from(value).map(SignUp::Session)
        } else {
            Ok(SignUp::ConfirmEmail)
        }
    }

    /// Renueva la sesión con el `refresh_token`.
    pub fn refresh(&self, session: &Session) -> Result<Session, SyncError> {
        let body = json!({ "refresh_token": session.refresh_token });
        match self.auth_post("token?grant_type=refresh_token", &body) {
            Ok(value) => session_from(value),
            // Un refresh rechazado significa que hay que volver a entrar.
            Err(SyncError::Rejected(m)) => Err(SyncError::Auth(m)),
            Err(e) => Err(e),
        }
    }

    /// Cierra la sesión en el servidor (si falla, da igual: localmente se olvida igual).
    pub fn sign_out(&self, session: &Session) {
        let _ = self
            .agent
            .post(&format!("{}/auth/v1/logout", self.url))
            .set("apikey", &self.key)
            .set("Authorization", &format!("Bearer {}", session.access_token))
            .call();
    }

    /// Acceso a los datos con `session`, que se renueva sola cuando hace falta.
    pub fn remote<'a>(&'a self, session: &'a mut Session) -> SupabaseRemote<'a> {
        SupabaseRemote {
            client: self,
            session,
        }
    }

    fn auth_post(&self, path: &str, body: &Value) -> Result<Value, SyncError> {
        let response = self
            .agent
            .post(&format!("{}/auth/v1/{path}", self.url))
            .set("apikey", &self.key)
            .send_json(body.clone());
        match response {
            Ok(r) => r
                .into_json()
                .map_err(|e| SyncError::Rejected(format!("respuesta inválida: {e}"))),
            Err(e) => Err(map_error(e, true)),
        }
    }
}

/// Datos de la cuenta con una sesión.
pub struct SupabaseRemote<'a> {
    client: &'a Supabase,
    session: &'a mut Session,
}

impl SupabaseRemote<'_> {
    /// La sesión (puede haberse renovado).
    pub fn session(&self) -> &Session {
        self.session
    }

    fn ensure_fresh(&mut self) -> Result<(), SyncError> {
        let now = crate::now_ms() / 1000;
        if self.session.expires_at - REFRESH_MARGIN_S <= now {
            *self.session = self.client.refresh(self.session)?;
        }
        Ok(())
    }

    /// Hace una petición; si el token venció (401), renueva una vez y reintenta.
    fn request(
        &mut self,
        build: impl Fn(&Supabase, &str) -> Result<ureq::Response, ureq::Error>,
    ) -> Result<ureq::Response, SyncError> {
        self.ensure_fresh()?;
        match build(self.client, &self.session.access_token) {
            Err(ureq::Error::Status(401, _)) => {
                *self.session = self.client.refresh(self.session)?;
                build(self.client, &self.session.access_token).map_err(|e| map_error(e, false))
            }
            other => other.map_err(|e| map_error(e, false)),
        }
    }
}

#[derive(Deserialize)]
struct ItemRow {
    id: String,
    parent_id: Option<String>,
    data: Value,
    updated_at: String,
    deleted: bool,
    server_updated_at: String,
}

// Los cierres devuelven `ureq::Error`, que es grande; es de `ureq` y solo vive un instante.
#[allow(clippy::result_large_err)]
impl Remote for SupabaseRemote<'_> {
    fn pull(&mut self, since: Option<i64>) -> Result<Vec<RemoteRow>, SyncError> {
        let since = since.map(format_ms).transpose()?;
        let mut rows = Vec::new();
        // Paginar por posición: un lote subido de una vez comparte la misma hora de servidor.
        loop {
            let offset = rows.len();
            let response = self.request(|c, token| {
                let mut req = c
                    .agent
                    .get(&format!("{}/rest/v1/items", c.url))
                    .set("apikey", &c.key)
                    .set("Authorization", &format!("Bearer {token}"))
                    .query(
                        "select",
                        "id,parent_id,data,updated_at,deleted,server_updated_at",
                    )
                    .query("order", "server_updated_at.asc,id.asc")
                    .query("limit", &PAGE.to_string())
                    .query("offset", &offset.to_string());
                if let Some(s) = &since {
                    req = req.query("server_updated_at", &format!("gt.{s}"));
                }
                req.call()
            })?;
            let page: Vec<ItemRow> = response
                .into_json()
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

    fn push(&mut self, rows: &[SyncRow]) -> Result<(), SyncError> {
        for chunk in rows.chunks(PAGE) {
            let body = chunk.iter().map(row_json).collect::<Result<Vec<_>, _>>()?;
            let body = Value::Array(body);
            self.request(|c, token| {
                c.agent
                    .post(&format!("{}/rest/v1/items", c.url))
                    .set("apikey", &c.key)
                    .set("Authorization", &format!("Bearer {token}"))
                    .set("Prefer", "resolution=merge-duplicates,return=minimal")
                    .send_json(body.clone())
            })?;
        }
        Ok(())
    }
}

/// Cuerpo JSON de una fila para subirla (sin `user_id`: lo pone el servidor).
pub(crate) fn row_json(row: &SyncRow) -> Result<Value, SyncError> {
    let data: Value = serde_json::from_str(&row.data)
        .map_err(|e| SyncError::Rejected(format!("nodo dañado {}: {e}", row.id)))?;
    Ok(json!({
        "id": row.id.to_string(),
        "parent_id": row.parent_id.map(|p| p.to_string()),
        "data": data,
        "updated_at": format_ms(row.updated_at)?,
        "deleted": row.deleted,
    }))
}

fn to_remote(item: ItemRow) -> Result<RemoteRow, SyncError> {
    let bad = |what: &str| SyncError::Rejected(format!("fila inválida del servidor ({what})"));
    let id: NodeId = item.id.parse().map_err(|_| bad("id"))?;
    let parent_id = item
        .parent_id
        .map(|p| p.parse::<NodeId>())
        .transpose()
        .map_err(|_| bad("parent_id"))?;
    Ok(RemoteRow {
        row: SyncRow {
            id,
            parent_id,
            data: item.data.to_string(),
            updated_at: parse_ms(&item.updated_at).ok_or_else(|| bad("updated_at"))?,
            deleted: item.deleted,
        },
        server_updated_at: parse_ms(&item.server_updated_at)
            .ok_or_else(|| bad("server_updated_at"))?,
    })
}

fn session_from(value: Value) -> Result<Session, SyncError> {
    let token: TokenResponse = serde_json::from_value(value)
        .map_err(|e| SyncError::Rejected(format!("respuesta de sesión inválida: {e}")))?;
    Ok(Session {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: crate::now_ms() / 1000 + token.expires_in,
        user_id: token.user.id,
        email: token.user.email.unwrap_or_default(),
    })
}

/// Milisegundos → texto RFC 3339 ("2026-11-07T17:32:41.526Z").
pub(crate) fn format_ms(ms: i64) -> Result<String, SyncError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .ok_or_else(|| SyncError::Rejected(format!("fecha inválida: {ms}")))
}

/// Texto RFC 3339 de Postgres → milisegundos.
pub(crate) fn parse_ms(text: &str) -> Option<i64> {
    let t = OffsetDateTime::parse(text, &Rfc3339).ok()?;
    i64::try_from(t.unix_timestamp_nanos() / 1_000_000).ok()
}

/// Traduce un error HTTP a uno para mostrar. `auth`: el error viene de iniciar sesión o crear
/// cuenta (los 400 son por datos del usuario).
fn map_error(error: ureq::Error, auth: bool) -> SyncError {
    match error {
        ureq::Error::Transport(t) => SyncError::Offline(t.to_string()),
        ureq::Error::Status(status, response) => {
            let body: Value = response.into_json().unwrap_or(Value::Null);
            let code = body
                .get("error_code")
                .or_else(|| body.get("error"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let message = body
                .get("msg")
                .or_else(|| body.get("message"))
                .or_else(|| body.get("error_description"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if status >= 500 {
                return SyncError::Unavailable(format!("error {status}"));
            }
            let friendly = match code {
                "invalid_credentials" | "invalid_grant" if auth => {
                    Some("Correo o contraseña incorrectos.")
                }
                "email_not_confirmed" => {
                    Some("Falta confirmar el correo: revisa tu bandeja (y la de spam).")
                }
                "user_already_exists" | "email_exists" => {
                    Some("Ya existe una cuenta con ese correo. Usa «Entrar».")
                }
                "weak_password" => Some("La contraseña es muy débil (usa al menos 6 caracteres)."),
                "email_address_invalid" | "validation_failed" => Some("El correo no es válido."),
                "over_email_send_rate_limit" | "over_request_rate_limit" => {
                    Some("Demasiados intentos seguidos. Espera un rato y vuelve a intentar.")
                }
                "refresh_token_not_found" | "refresh_token_already_used" | "session_not_found" => {
                    Some("La sesión venció. Vuelve a entrar.")
                }
                _ => None,
            };
            let text = friendly.map(str::to_owned).unwrap_or_else(|| {
                if message.is_empty() {
                    format!("el servidor respondió {status}")
                } else {
                    message
                }
            });
            if status == 401 || status == 403 {
                SyncError::Auth(text)
            } else {
                SyncError::Rejected(text)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_round_trip() {
        let ms = 1_793_986_361_526;
        let text = format_ms(ms).unwrap();
        assert_eq!(parse_ms(&text), Some(ms));
        // Formato de Postgres (microsegundos y +00:00).
        let postgres = parse_ms("2026-11-07T17:32:41.526795+00:00").unwrap();
        assert_eq!(Some(postgres), parse_ms("2026-11-07T14:32:41.526-03:00"));
        assert_eq!(format_ms(postgres).unwrap(), "2026-11-07T17:32:41.526Z");
        assert_eq!(parse_ms("no"), None);
    }

    #[test]
    fn upload_body_has_no_user_id() {
        let node = notable_core::Node::new("Ramo", None);
        let row = SyncRow::from_node(&node, 1_000);
        let body = row_json(&row).unwrap();
        assert_eq!(body["id"], node.id.to_string());
        assert_eq!(body["data"]["name"], "Ramo");
        assert_eq!(body["updated_at"], "1970-01-01T00:00:01Z");
        assert!(body.get("user_id").is_none());
    }
}
