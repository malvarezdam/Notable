//! Esquema de la base y migraciones por versión (`PRAGMA user_version`).
//!
//! Cada migración lleva la base de la versión `i` a la `i + 1`. Para cambiar el esquema se
//! agrega una migración al final; nunca se modifica una que ya se publicó.

use rusqlite::Connection;

use crate::StoreError;

const MIGRATIONS: &[&str] = &[
    // 1: tabla de nodos. `data` es el nodo en JSON; `parent_id` y `deleted` se repiten como
    // columnas para poder consultarlos (y sincronizar) sin leer el JSON.
    "CREATE TABLE items (
        id         TEXT PRIMARY KEY NOT NULL,
        parent_id  TEXT,
        data       TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        deleted    INTEGER NOT NULL DEFAULT 0,
        dirty      INTEGER NOT NULL DEFAULT 1
    );
    CREATE INDEX items_dirty ON items (dirty) WHERE dirty = 1;",
];

/// Versión de esquema que entiende esta versión de la app.
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// Aplica las migraciones que falten. Cada una va en su propia transacción.
pub fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let current: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as u32 + 1)?;
        tx.commit()?;
    }
    Ok(())
}
