# Cómo contribuir

¡Gracias por querer ayudar con Notable! Este es un proyecto personal y abierto: cualquier aporte
es bienvenido, desde avisar de un error hasta mandar código.

## Reportar un error o pedir algo

Abre un [issue](https://github.com/malvarezdam/Notable/issues/new/choose) y elige la plantilla:

- **Error:** qué hiciste, qué esperabas y qué pasó. Indica el sistema (Windows, Linux, Android)
  y la versión de Notable. Si es un cálculo, incluye las notas, pesos y configuración: con eso
  se puede reproducir.
- **Idea:** qué te gustaría poder hacer y por qué. Muchas reglas de notas cambian según la
  universidad; cuéntanos cómo funciona en la tuya.

Para problemas de **seguridad** no abras un issue público: sigue [SECURITY.md](SECURITY.md).

## Principios del proyecto

Antes de proponer cambios, conviene conocer las reglas de [PLAN.md](PLAN.md):

1. **Todo configurable y todo opcional.** Ninguna regla de una institución en particular queda
   fija: se ofrece como opción que el usuario activa (y viene apagada).
2. **Por defecto, lo simple.** Promedio simple; los pesos y las reglas se activan a pedido.
3. **Local primero.** La app funciona completa sin internet y sin cuenta.
4. **El cálculo es lo más importante.** Vive en `crates/notable-core`, sin interfaz ni base de
   datos, y **toda regla nueva lleva pruebas**.
5. **Estética simple.** Pocas pantallas y pocos botones visibles.
6. **Nada secreto en el repo.** Solo la URL y la clave *publishable* de Supabase.

## Preparar el entorno

Necesitas Rust estable, el objetivo `wasm32-unknown-unknown`, [Trunk](https://trunkrs.dev) y la
CLI de Tauri 2:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
cargo install tauri-cli --version "^2"
```

En Linux, además, las dependencias de Tauri (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`,
`librsvg2-dev`). Para Android, ver [docs/CONFIGURACION.md](docs/CONFIGURACION.md).

```bash
cd app/src-tauri && cargo tauri dev    # la app con recarga automática
```

## Antes de mandar un pull request

Lo mismo que revisa el CI:

```bash
cargo fmt --all
cargo clippy --workspace --exclude notable-ui --all-targets -- -D warnings
cargo clippy -p notable-ui --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace --exclude notable-ui
```

- **Idioma:** la interfaz, los mensajes y la documentación van en **español**; el código y los
  identificadores, en **inglés**. Los comentarios del código, en español.
- Un pull request por tema, con una descripción de qué cambia y por qué.
- Si cambias el formato de los datos guardados, que los datos viejos sigan cargando (campos
  nuevos con valor por defecto) y agrega una migración en `crates/notable-store` si toca SQLite.

## Licencia de los aportes

Al contribuir aceptas que tu aporte se publique bajo la [licencia MIT](LICENSE) del proyecto.
