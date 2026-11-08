//! Lo que no depende de cómo se hagan las peticiones: formato de las filas, de la sesión y de
//! los errores. Lo comparten el cliente nativo y el del navegador.

use notable_core::NodeId;
use notable_store::SyncRow;
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{RemoteRow, Session, SyncError};

/// Mensaje cuando el correo o la contraseña no corresponden.
pub(crate) const WRONG_CREDENTIALS: &str = "Correo o contraseña incorrectos.";

/// Largo mínimo de contraseña que acepta el proyecto de Supabase.
pub(crate) const MIN_PASSWORD: usize = 6;

pub(crate) const WEAK_PASSWORD: &str = "La contraseña es muy débil (usa al menos 6 caracteres).";

/// Resultado de crear una cuenta.
#[derive(Debug, PartialEq)]
pub enum SignUp {
    /// La cuenta quedó lista y con sesión iniciada.
    Session(Session),
    /// Hay que confirmar el correo antes de entrar.
    ConfirmEmail,
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

#[derive(Deserialize)]
pub(crate) struct ItemRow {
    id: String,
    parent_id: Option<String>,
    data: Value,
    updated_at: String,
    deleted: bool,
    server_updated_at: String,
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

pub(crate) fn to_remote(item: ItemRow) -> Result<RemoteRow, SyncError> {
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

pub(crate) fn session_from(value: Value) -> Result<Session, SyncError> {
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

/// Traduce una respuesta de error del servidor a uno para mostrar. `auth`: viene de iniciar
/// sesión o crear cuenta (los 400 son por datos del usuario).
pub(crate) fn http_error(status: u16, body: &Value, auth: bool) -> SyncError {
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
        "invalid_credentials" | "invalid_grant" if auth => Some(WRONG_CREDENTIALS),
        "email_not_confirmed" => {
            Some("Falta confirmar el correo: revisa tu bandeja (y la de spam).")
        }
        "user_already_exists" | "email_exists" => {
            Some("Ya existe una cuenta con ese correo. Usa «Entrar».")
        }
        "same_password" => Some("La contraseña nueva debe ser distinta de la actual."),
        "otp_expired" => Some("El código no es válido o ya venció. Pide uno nuevo."),
        "weak_password" => Some(WEAK_PASSWORD),
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
