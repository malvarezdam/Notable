# Notable

Gestor de notas y calificaciones para **Windows, Android y la web** (también iPhone), escrito en Rust.
Arma tu carrera, tus ramos y sus evaluaciones como quieras, ingresa las notas y Notable calcula
tus promedios y cuánto necesitas para llegar a la nota que buscas.

<p align="center">
  <img src="docs/capturas/ramo.png" alt="Un ramo con sus notas y el promedio" width="280">
  &nbsp;&nbsp;
  <img src="docs/capturas/cuenta.png" alt="Panel de cuenta opcional" width="280">
</p>

**Úsala en:** [el navegador](https://malvarezdam.github.io/Notable/) (también en iPhone) ·
[Windows y Android](https://github.com/malvarezdam/Notable/releases/latest)

## Funciones

### Organiza a tu manera
- **Todo es un árbol:** carrera, año, semestre, ramo, sección o evaluación son niveles a los que
  tú les pones el nombre, con tantos niveles como necesites.
- **Mueve lo que quieras** con el botón **⇄**: una sección pasa a otro lugar con todo lo que
  tiene adentro, sin borrar nada.
- **Reordena y renombra** con **✎**. Si borras algo, puedes deshacerlo al momento.

### Calcula tus notas
- **Promedio simple** por defecto: no hay que configurar nada para empezar.
- **Ponderado** con el botón **Ponderar**, incluso con pesos respecto a otro nivel ("vale 20 % del
  ramo"), y **promedio por créditos**.
- **Nota parcial y final:** cada nivel muestra su promedio con lo que hay hasta completar todo.
- **Nota objetivo:** te dice cuánto necesitas en lo que falta ("Necesitas 4,3 en el Certamen 2"),
  o si ya está asegurado o no se puede alcanzar. Toma en cuenta todas tus reglas.
- **Avisos** sin bloquear: pesos que no suman 100 %, notas fuera de la escala y otros problemas.

### Adáptala a tu universidad
- **Escalas a tu medida:** 1,0–7,0, 0–100 o la que quieras, con nota de aprobación, decimales,
  aproximar o truncar. Cada nivel la hereda o usa la suya, y se convierte entre escalas.
- **Redondeo** en cada nivel o solo donde lo pidas.
- **Reglas opcionales**, todas apagadas hasta que las actives:

| Regla | Qué hace |
|---|---|
| Eliminar las peores | Las N notas más bajas no cuentan |
| Reemplazo | Un examen o una recuperativa reemplaza a la peor nota o a una en particular |
| Eximición | Una evaluación no se rinde si el promedio de lo demás alcanza |
| Nota mínima | Sin ese mínimo no se aprueba el nivel de arriba, con tope de nota opcional |
| Factor | Una nota, como el laboratorio, no se promedia: multiplica la nota de arriba por un factor dentro de un rango, y puede subirla o bajarla |

### Tus datos, en tus dispositivos
- **Funciona sin cuenta y sin internet:** cada cambio se guarda al instante en el dispositivo.
- **Cuenta opcional** para sincronizar PC, teléfono y web. Si cambias lo mismo en dos lados, gana
  el cambio más reciente.
- **Contraseña:** puedes cambiarla desde tu cuenta o recuperarla con un código por correo.
- **Respaldo en archivo:** exporta todo a un `.json` e impórtalo después, sin cuenta. Importar
  agrega sin borrar lo que ya tienes.
- **Tema** claro, oscuro o el del sistema, y diseño para teléfono y PC.

## Usar en el navegador o en iPhone

Abre **<https://malvarezdam.github.io/Notable/>**: es la misma app, sin descargar nada. Reconoce
sola si estás en un teléfono o en un PC.

**iPhone:** ábrela en Safari, pulsa **Compartir → Añadir a pantalla de inicio** y queda con su
ícono, como una app. Con la misma cuenta ves las mismas notas que en el PC y en Android. Sin cuenta,
las notas quedan solo en ese navegador; conviene usar cuenta, porque el navegador puede borrar
sus datos.

## Descargar

Descarga la última versión desde [Releases](https://github.com/malvarezdam/Notable/releases/latest).
No hay que instalar nada en el PC:

| Sistema | Archivo |
|---|---|
| Windows | `Notable_…_x64-portable.exe`: se abre con doble clic, desde donde quieras |
| Android | `Notable-…-android-arm64.apk` (casi todos los teléfonos; `…-android-arm.apk` para los muy antiguos) |

**Windows:** si aparece "Windows protegió tu PC", pulsa *Más información → Ejecutar de todas
formas* (la app no está firmada con un certificado de pago). Para actualizar, reemplaza el `.exe`
por el nuevo: tus notas se conservan.

**Android:** abre el `.apk` en el teléfono y permite "instalar apps desconocidas" para la app
desde la que lo abriste (navegador, Drive…). Para actualizar, instala el nuevo encima: tus notas
se conservan.

**Linux:** no hay descarga lista; se puede [compilar](#compilar).

## Cómo se usa

1. **Agrega** lo que quieras organizar: una carrera, un año, un semestre o directamente un ramo.
   Entra a cada uno y agrega sus evaluaciones o secciones (por ejemplo, "Certámenes" con sus
   certámenes adentro). Los nombres los pones tú: todo es un árbol. ¿Lo pusiste en el lugar
   equivocado? Con **⇄** lo mueves a otro lado con todo lo que tiene adentro.
2. **Ingresa las notas** en las evaluaciones. Cada nivel muestra su promedio con lo que hay
   ("nota parcial") hasta que estén todas ("nota final").
3. ¿Las evaluaciones no valen lo mismo? Pulsa **Ponderar** y escribe el % de cada una.
4. En **⚙ Ajustes** de cada nivel: nota objetivo, escala, redondeo, créditos y, plegadas,
   las **reglas opcionales**. Por ejemplo, si el laboratorio multiplica la nota final
   (factor entre 0,8 y 1,1), ábrelo y activa **Multiplica la nota de arriba**.
5. Opcional: pulsa **☁** para crear una cuenta y sincronizar tus dispositivos. Sin cuenta, todo
   se guarda solo en el dispositivo. Ahí mismo puedes **cambiar tu contraseña** o, antes de entrar,
   pulsar **Olvidé mi contraseña** para recibir un código por correo.
6. Respaldo: en **☁ → Respaldo en archivo** puedes **exportar** todo a un archivo `.json` y
   **importarlo** después (o en otro dispositivo). Importar agrega sin borrar lo que ya tienes.

## Compilar

Requiere Rust (estable), el objetivo `wasm32-unknown-unknown`, [Trunk](https://trunkrs.dev) y
la CLI de Tauri 2 (`cargo install tauri-cli --version "^2"`).

```bash
cargo test --workspace --exclude notable-ui   # pruebas
cd app/src-tauri && cargo tauri dev           # la app en modo desarrollo
cd app && trunk serve --features web          # la versión web en modo desarrollo
cd app/src-tauri && cargo tauri build --no-bundle  # ejecutable de escritorio (target/release)
```

Para Android y la configuración de Supabase, ver [docs/CONFIGURACION.md](docs/CONFIGURACION.md).
El plan de desarrollo y las decisiones están en [PLAN.md](PLAN.md).

| Carpeta | Qué hay |
|---|---|
| `crates/notable-core` | Modelo (árbol de nodos), cálculo, nota mínima y reglas. Sin IO. |
| `crates/notable-store` | Guardado local (SQLite en la app, almacenamiento del navegador en la web) y respaldo en JSON. |
| `crates/notable-sync` | Cuenta y sincronización con Supabase. |
| `crates/notable-state` | Estado y operaciones, comunes a la app y a la web. |
| `crates/notable-api` | Tipos que intercambian la app y la interfaz. |
| `app/` | Interfaz (Leptos, WebAssembly), también la versión web. `app/src-tauri/`: la app (Tauri 2, escritorio y Android). |
| `supabase/schema.sql` | Esquema del servidor. |

## Contribuir

Errores, ideas y pull requests son bienvenidos: ver [CONTRIBUTING.md](CONTRIBUTING.md) y el
[código de conducta](CODE_OF_CONDUCT.md). Los problemas de seguridad se reportan en privado
([SECURITY.md](SECURITY.md)). Qué datos guarda la app y dónde: [PRIVACY.md](PRIVACY.md).
Historial de versiones: [CHANGELOG.md](CHANGELOG.md).

## Derechos de autor y licencia

Copyright © 2026 malvarezdam. Notable es software libre bajo la [licencia MIT](LICENSE): puedes
usarlo, copiarlo, modificarlo y distribuirlo, también con fines comerciales, siempre que
conserves el aviso de copyright y la licencia. Se entrega "tal cual", sin garantías: revisa tus
notas oficiales con tu institución.

Notable usa bibliotecas de terceros (Tauri, Leptos, SQLite, entre otras), cada una con su propia
licencia (en su mayoría MIT o Apache-2.0); sus avisos se conservan en sus paquetes.
