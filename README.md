# Notable

Gestor de notas y calificaciones para PC y Android, escrito en Rust.

- **Local primero:** funciona completo sin internet y sin cuenta.
- **Simple por defecto:** los promedios son simples; el botón **Ponderar** activa pesos.
- **Todo configurable:** escala, decimales, redondeo, nota objetivo y reglas opcionales
  (eliminar la peor nota, eximición, requisitos mínimos, créditos…), todas apagadas hasta que las actives.
- **Sincronización opcional** entre dispositivos con una cuenta.

> 🚧 En desarrollo. El plan está en [PLAN.md](PLAN.md).

## Compilar

Pruebas (cálculo, guardado y app):

```bash
cargo test --workspace
```

La app (Tauri 2 + Leptos) necesita una vez:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
cargo install tauri-cli --version "^2" --locked
```

Y luego, desde la carpeta `app/`:

```bash
cargo tauri dev
```

Los datos se guardan en `%APPDATA%\io.github.malvarezdam.notable\notable.db` (Windows).

## Licencia

[MIT](LICENSE)
