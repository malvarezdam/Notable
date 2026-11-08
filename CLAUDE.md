# Notable

Gestor de notas/calificaciones en Rust para Windows, Android y web (Linux solo compilando). Proyecto personal open
source: <https://github.com/malvarezdam/Notable> (licencia MIT).

## Estado

- **Versión publicada: 1.0.0** (tag `v1.0.0`, GitHub Releases: `.exe` portable de Windows sin instalador, APK `arm64` y
  `arm` firmados). Las Fases 0–10 de [PLAN.md](PLAN.md) están
  hechas; su "Registro de decisiones" explica el porqué de cada elección: leerlo antes de cambiar
  algo de fondo.
- No subir la versión ni crear tags/releases sin que el usuario lo pida. Para publicar, ver
  `docs/CONFIGURACION.md` §4.2 (versión en `Cargo.toml` y `app/src-tauri/tauri.conf.json`, luego tag).
- Pendiente del lado del usuario: probar la sincronización con una cuenta real (PC ↔ teléfono),
  instalar el APK en su teléfono.
- Versión web publicada en <https://malvarezdam.github.io/Notable/> con cada push a `main`
  (`pages.yml`, GitHub Pages por Actions).
- Repo de GitHub configurado: Wiki desactivada, reporte privado de
  vulnerabilidades y Dependabot (alertas y arreglos) activados, secretos solo los de firma de Android.

## Reglas

- Toda regla de cálculo es **opcional y configurable por el usuario**; por defecto, promedio
  simple. No preguntar "¿cómo lo hace tu universidad?": ofrecer la opción (apagada por defecto).
- La lógica de cálculo vive en `notable-core` (sin IO) y **toda regla nueva lleva pruebas**.
- Idioma: interfaz, mensajes, documentación y comentarios en español; código e identificadores
  en inglés.
- Nunca subir secretos: solo la URL y la clave *publishable* de Supabase van en el repo
  (`crates/notable-sync/src/lib.rs`). La clave de firma de Android está fuera del repo, en
  `C:\Users\Victus\.android-keys\` (y como secretos de GitHub Actions).
- Datos guardados: campos nuevos con `#[serde(default)]`; cambios de SQLite como migración nueva
  en `crates/notable-store/src/schema.rs` (nunca editar una ya publicada).
- Acciones irreversibles o hacia afuera (borrar, ajustes del repo, secretos, releases): confirmar
  con el usuario antes.

## Estructura

| Carpeta | Qué hay |
|---|---|
| `crates/notable-core` | Árbol de nodos, cálculo (`calc.rs`), nota mínima (`goal.rs`), reglas opcionales (`rules.rs`). |
| `crates/notable-store` | SQLite (`rusqlite` bundled, `native.rs`), guardado web (`kv.rs`/`web.rs`, localStorage), migraciones, respaldo JSON. Las reglas de guardado deben ser iguales en ambos. |
| `crates/notable-sync` | Supabase: `supabase.rs` (`ureq`), `web.rs` (`fetch`), `protocol.rs` (común); sincronización bajar→aplicar→subir. |
| `crates/notable-state` | Estado de la app (árbol + guardado + pantallas), común a nativo y web; se prueba directo. |
| `crates/notable-api` | Tipos que intercambian app e interfaz (datos ya formateados). |
| `app/` | Interfaz Leptos 0.8 (CSR, WebAssembly, Trunk). Con la feature `web` (`src/web.rs`, `web/`) corre sin Tauri. |
| `app/src-tauri/` | App Tauri 2: comandos (`lib.rs`), hilo de sincronización (`sync.rs`). `gen/android` se versiona (firma en `app/build.gradle.kts`, márgenes en `MainActivity.kt`). |
| `supabase/schema.sql` | Tabla `items` con RLS (proyecto `augvbftjwvysrozyxvbh`). |
| `.github/workflows/` | `ci.yml` (fmt, clippy, pruebas), `release.yml` (tag `v*` → release) y `pages.yml` (web). |

## Comandos

```bash
cargo test --workspace --exclude notable-ui
cargo clippy --workspace --exclude notable-ui --all-targets -- -D warnings
cargo clippy -p notable-ui --target wasm32-unknown-unknown -- -D warnings
cargo clippy -p notable-ui --target wasm32-unknown-unknown --features web -- -D warnings
cd app/src-tauri && cargo tauri dev
cd app/src-tauri && cargo tauri android build --apk --target aarch64   # requiere Modo para desarrolladores de Windows
```

- Probar la interfaz: el control de escritorio no alcanza la ventana de `tauri dev`; usar el
  emulador Android (`Medium_Phone_API_37.0`, `emulator -no-window` + `adb screencap`/`input`).
- La configuración que hace el usuario (Supabase, Android, secretos) está en
  [docs/CONFIGURACION.md](docs/CONFIGURACION.md).
