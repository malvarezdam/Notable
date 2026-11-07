# Notable — Plan de desarrollo

Plan de trabajo para Claude. **Una fase por sesión** (o menos, si la fase es grande).
Al terminar una fase: marcar sus casillas, anotar decisiones nuevas en "Registro de decisiones"
y dejar el repositorio compilando y con las pruebas pasando.

## Principios (no negociables)

1. **Todo configurable y todo opcional.** Ante cualquier duda de reglas (examen final, eximición,
   requisitos mínimos, créditos, redondeo por nivel, etc.) la respuesta es: el usuario puede activarlo
   y configurarlo, o simplemente no usarlo. Nada de reglas fijas de una institución.
2. **Por defecto, lo simple.** Los hijos de un nodo se promedian de forma simple. El botón
   **"Ponderar"** activa pesos manuales. Las funciones opcionales están apagadas hasta que el
   usuario las active.
3. **Local primero.** La app funciona completa sin internet y sin cuenta. La sincronización es una capa aparte.
4. **El cálculo es lo más importante.** La lógica vive en `notable-core`, sin interfaz ni base de
   datos, y toda regla nueva lleva pruebas.
5. **Estética simple.** Pocas pantallas, pocos botones visibles, configuración avanzada escondida.
6. **Nada secreto en el repo.** Solo la URL y la clave *publishable/anon* de Supabase. Nunca la
   `service_role`/secret ni la contraseña de la base de datos.

## Arquitectura

```
Notable/
├── crates/
│   ├── notable-core/    # Modelo (árbol), cálculo, nota mínima. Rust puro + serde. Sin IO.
│   ├── notable-store/   # SQLite local (rusqlite): guardar/cargar el árbol, marcas de cambio.
│   └── notable-sync/    # Cliente Supabase (reqwest): login, subir/bajar cambios.
├── app/                 # Tauri 2 (escritorio + Android), UI con Leptos.
├── supabase/schema.sql  # Esquema remoto (genérico: tabla `items` con datos JSON).
└── .github/workflows/   # CI y releases.
```

Modelo: todo es un **nodo** de un árbol (carrera, año, semestre, ramo, sección, tipo, evaluación
son solo nombres que pone el usuario). Cada nodo puede tener: hijos, nota (si es hoja), peso, modo
(promedio simple / ponderado), configuración de nota heredable (escala min-max, nota de aprobación,
decimales, truncar/aproximar, redondear en este nivel sí/no), objetivo opcional y reglas opcionales.

---

## Fases

### Fase 0 — Preparación del repositorio
- [x] `git init`, `.gitignore` (target/, node_modules/, dist/, gen/ de Tauri), licencia MIT, README corto.
- [x] Cargo workspace con `crates/notable-core` vacío y compilando.
- [x] Repo en GitHub y primer push: se usó el repo existente `malvarezdam/Notable` (su versión web anterior se reemplazó, con respaldo local).
**Listo cuando:** `cargo test` pasa en el workspace y el repo existe en GitHub.

### Fase 1 — Modelo de datos (`notable-core`)
- [x] Tipos: `Node`, `NodeId` (UUID), `GradeConfig` (escala, aprobación, decimales, `Rounding::{Round, Truncate}`), `Aggregation::{Simple, Weighted}`.
- [x] Árbol en memoria con operaciones: crear, mover, renombrar, borrar (borrado lógico), editar nota.
- [x] Herencia de `GradeConfig` (un nodo usa la de su ancestro más cercano que la defina).
- [x] Serialización serde (JSON) de cada nodo por separado (sirve para SQLite y sync).
**Listo cuando:** hay pruebas de construcción del árbol, herencia y serialización ida y vuelta.

### Fase 2 — Motor de cálculo
- [x] Promedio de cada nodo: simple o ponderado, ignorando hijos sin nota (nota "parcial actual").
- [x] Pesos expresados respecto a un ancestro ("vale 20% del curso") → conversión a peso relativo al padre.
- [x] Validaciones como avisos (no errores): pesos que no suman 100 %, nota fuera de escala, pesos contradictorios.
- [x] Redondeo/truncado configurable: solo en la nota final del nodo con objetivo, o también en cada nivel (opción).
- [x] Conversión entre escalas cuando un hijo usa una escala distinta a la del padre (lineal).
**Listo cuando:** pruebas con casos reales (ramo USM 0–100, ramo 1–7, mezcla de escalas, redondeo intermedio).

### Fase 3 — Nota objetivo y nota mínima necesaria
- [x] Objetivo opcional por nodo (normalmente el ramo).
- [x] Si falta exactamente 1 nota: nota mínima necesaria. Despeje lineal cuando no hay redondeo intermedio;
      búsqueda sobre los valores posibles de la escala (según decimales) cuando sí lo hay.
      *(Se usa siempre la búsqueda; ver registro.)*
- [x] Resultados: `Necesitas(x)`, `YaCumplido`, `NoAlcanzable`.
- [x] Extra: si faltan varias notas, promedio necesario en lo que falta (asumiendo la misma nota en todas).
**Listo cuando:** pruebas de casos borde (exacto en el límite, truncado vs aproximado, ya cumplido, imposible).

### Fase 4 — Reglas opcionales (todas apagadas por defecto)
- [x] Eliminar la(s) peor(es) nota(s) de un nodo.
- [x] Examen/nota que reemplaza a la peor nota (o a una nota elegida).
- [x] Eximición: si el promedio ≥ X, una evaluación (p. ej. examen) no se rinde/no cuenta.
- [x] Requisito mínimo: "este nodo debe tener ≥ X para aprobar el padre" (muestra aviso/estado reprobado).
- [x] Promedio de semestre/año/carrera ponderado por créditos (campo "créditos" opcional en el ramo).
- [x] Integrar las reglas con la Fase 3 (la nota mínima las respeta).
**Listo cuando:** cada regla tiene pruebas encendida y apagada.

### Fase 5 — Guardado local (`notable-store`)
- [x] SQLite con `rusqlite` (feature `bundled`): tabla `items(id, parent_id, data JSON, updated_at, deleted, dirty)`.
- [x] Migraciones simples por versión.
- [x] Cargar el árbol completo al iniciar; guardar cada cambio al instante (`dirty = 1`).
- [x] Exportar/importar a archivo JSON (respaldo manual, sin cuenta).
**Listo cuando:** pruebas de guardar → cerrar → abrir dan el mismo árbol.

### Fase 6 — App base (Tauri 2 + Leptos)
- [x] Proyecto Tauri 2 en `app/` usando los crates; comandos Tauri para leer/modificar el árbol.
- [x] Pantalla principal: navegación por el árbol (lista + migas de pan), crear/renombrar/borrar nodos, ingresar notas.
- [x] Cada nodo muestra su promedio actual.
**Listo cuando:** en Windows se puede armar un ramo completo e ingresar notas, y se guarda al cerrar.

### Fase 7 — UI completa
- [x] Botón "Ponderar" visible en cada nodo con hijos; edición de pesos (y "respecto a" otro nivel).
- [x] Panel de configuración de nota (escala, decimales, truncar/aproximar), heredable.
- [x] Objetivo y tarjeta "Necesitas X en …".
- [x] Panel "Reglas opcionales" (Fase 4), plegado por defecto.
- [x] Diseño adaptable a teléfono, tema claro/oscuro.
**Listo cuando:** se pueden usar todas las funciones de Fases 1–4 desde la interfaz.

### Fase 8 — Cuenta y sincronización (`notable-sync`)
- [x] Requiere que el usuario haya hecho `docs/CONFIGURACION.md` (proyecto Supabase + `schema.sql`).
- [x] Login/registro con correo y contraseña (Supabase Auth REST), guardar la sesión y refrescar el token.
- [x] Subir los `dirty` (upsert), bajar cambios con `server_updated_at` mayor al último sync; gana el `updated_at` más reciente.
- [x] Sincronizar al abrir, al volver a tener internet y cada pocos minutos; indicador de estado.
- [x] Primer login en un dispositivo con datos locales: fusionar (no borrar nada).
- [x] Manejar proyecto pausado/sin conexión sin romper la app.
**Listo cuando:** cambios hechos en PC aparecen en otra instalación y viceversa, también tras trabajar sin conexión. **Falta:** probarlo con una cuenta real (lo hace el usuario: crear cuenta en la app y confirmar el correo).

### Fase 9 — Android
- [x] Requiere herramientas Android del usuario (ver `docs/CONFIGURACION.md`, parte 3).
- [x] `tauri android init`, ajustar UI táctil, compilar APK firmado (clave guardada fuera del repo).
**Listo cuando:** el APK se instala en el teléfono del usuario y sincroniza con el PC. **Probado en el emulador** (Android 37): abre, calcula, guarda al cerrar y se conecta a Supabase por HTTPS. **Falta:** instalarlo en el teléfono y probar la sincronización con una cuenta real.

### Fase 10 — CI y publicación
- [x] GitHub Actions: `cargo test` + clippy en cada push.
- [x] Workflow de release (al crear un tag `v*`): instalador Windows, AppImage/deb Linux, APK Android → GitHub Releases.
- [x] Secretos del workflow (clave de firma Android) como GitHub Secrets.
- [x] README con capturas, instalación y cómo usar.
**Listo cuando:** crear un tag publica una release descargable. **Hecho:** `v1.0.0` publicada el 2026-11-07 (Windows .exe/.msi, Linux .AppImage/.deb, APK arm64 y arm firmados con la clave del usuario).

---

## Registro de decisiones
- 2026-11-07: Local primero (SQLite) + sincronización opcional con Supabase gratis; Tauri 2 + Leptos; distribución por GitHub Releases; iOS fuera por ahora.
- 2026-11-07: Promedio simple por defecto + botón "Ponderar". Todas las reglas extra son opcionales y configurables.
- 2026-11-07: El esquema remoto es genérico (`items` con `data jsonb`), así los cambios del modelo no obligan a tocar Supabase.
- 2026-11-07: Fase 0: workspace con edición 2024 y `resolver = "3"`; `Cargo.lock` se versiona (habrá binario). Rama `main`. Repo en GitHub (`malvarezdam/notable`) pospuesto a pedido del usuario hasta terminar en local.
- 2026-11-07: Fase 1: la `GradeConfig` se hereda completa (no campo por campo); si nadie la define se usa 1,0–7,0 / aprueba 4,0 / 1 decimal / aproximar. Hay atajos `SCALE_1_7` y `SCALE_0_100`.
- 2026-11-07: Fase 1: el borrado lógico marca solo el nodo; lo que cuelga de él queda oculto sin tocarse, así restaurar lo recupera todo y sincronizar el borrado es una sola fila.
- 2026-11-07: Fase 1: el orden entre hermanos es `position` en cada nodo. Se permiten varios nodos de nivel superior. Solo las hojas aceptan nota; una nota fuera de escala se permite (será aviso en la Fase 2).
- 2026-11-07: Fase 1: el core no lleva reloj ni `updated_at`; el `Tree` marca los nodos cambiados (`take_dirty`) y la capa de guardado pone la fecha. El JSON ignora campos desconocidos y completa los que faltan, para convivir entre versiones.
- 2026-11-07: Fase 2: por defecto no se redondea ningún nivel. Cada nodo puede pedirlo (`round_result`) y la configuración puede activar "redondear cada nivel" (`round_every_level`, heredable); un nodo puede excluirse. Las notas ingresadas nunca se redondean. Al redondear se limpia el ruido de punto flotante (4,45 → 4,5).
- 2026-11-07: Fase 2: los pesos son porcentajes. Si no suman 100 se avisa y se normaliza; un hijo sin peso en un nodo ponderado cuenta 0 (con aviso). La nota parcial usa solo los pesos de los hijos que tienen nota.
- 2026-11-07: Fase 2: conversión de escala lineal por defecto, con la opción `ByPass` (en dos tramos, la aprobación va a la aprobación: 55/100 → 4,0). La define la escala del hijo. Se convierte la nota ya redondeada del hijo.
- 2026-11-07: Fase 2: el peso "respecto a" un ancestro (`weight_base`) se divide por la fracción que vale el padre dentro de ese ancestro. Si la base deja de ser ancestro, o algún nivel intermedio vale 0, se usa como peso respecto al padre y se avisa. Los avisos nunca impiden calcular.
- 2026-11-07: Fase 3: el objetivo (`target`) va en la escala del nodo. Un nodo con objetivo redondea su nota final por defecto (es la que se compara); `round_result = Some(false)` lo apaga. Un objetivo fuera de escala se permite, con aviso (`TargetOutOfScale`), y resulta inalcanzable.
- 2026-11-07: Fase 3: la nota mínima siempre se busca (búsqueda binaria) sobre los valores que se pueden sacar en la escala según sus decimales, en vez de despejar: como la nota final nunca baja al subir una nota, el resultado es exacto con o sin redondeo intermedio, truncado o aproximado, `ByPass` y anidación, y siempre es una nota real (4,1 y no 4,03). Cuesta unas pocas decenas de cálculos del subárbol.
- 2026-11-07: Fase 3: `YaCumplido` significa asegurado (se cumple aunque lo que falta salga con la nota mínima); `NoAlcanzable` incluye "ya no falta nada y no se cumplió". Si faltan varias notas se supone el mismo lugar relativo en cada escala (la misma nota si comparten escala); se informa la nota de cada una en su escala.
- 2026-11-07: Fase 4: las reglas van en `Rules` dentro de cada nodo (se omite del JSON si está apagado). "Eliminar las peores" se pone en el padre; reemplazo, eximición y mínimo en el hijo afectado. Orden: reemplazos → eliminar peores → eximición (con la nota de presentación resultante) → promedio → requisitos mínimos. Las eximibles y los reemplazos no se eliminan ni son reemplazados. Un nodo no puede ser reemplazo y eximible a la vez.
- 2026-11-07: Fase 4: el reemplazo por defecto solo actúa si mejora y no cuenta como nota propia (ambas cosas configurables); con un hermano elegido sin nota, la toma (prueba recuperativa). "La peor" es la nota más baja, también en ponderado. Eximido, la evaluación no cuenta aunque tenga nota, salvo la opción "contarla si mejora".
- 2026-11-07: Fase 4: el requisito mínimo está en la escala del hijo y se compara con su nota ya reemplazada; si falla, el padre queda reprobado (`requirements_met`, `passed()`, aviso) y, opcionalmente, su nota se topa. Un hijo reprobado por sus propios mínimos no cumple el mínimo del padre. Sin nota, el requisito está pendiente.
- 2026-11-07: Fase 4: créditos como campo `credits` en el nodo y agregación `Credits`; un nodo sin créditos usa la suma de sus hijos (el semestre suma sus ramos). Los pesos por créditos se expresan como %.
- 2026-11-07: Fase 4: la nota mínima prueba todos los valores de la escala (hasta 20 000) en vez de búsqueda binaria, porque algunas reglas (peor nota en ponderado, eximición) hacen que la nota final pueda bajar al subir una nota; "cumplir" exige además los requisitos mínimos. Las evaluaciones que quedan eximidas no aparecen en "lo que falta".
- 2026-11-07: Fase 5: `notable-store` con `rusqlite` 0.41 (`bundled`, compila SQLite con MSVC sin instalar nada). Base en modo WAL. Migraciones en una lista con `PRAGMA user_version`, cada una en su transacción; una base de una versión más nueva se rechaza en vez de arriesgarla.
- 2026-11-07: Fase 5: la app llama a `Store::save(&mut tree)` tras cada cambio: guarda los nodos de `take_dirty` en una transacción con `dirty = 1`; si falla, los vuelve a marcar en el árbol (`Tree::mark_dirty`). `updated_at` es en milisegundos y nunca retrocede en una fila (si el reloj se atrasa avanza 1 ms), pensando en "gana el más reciente" de la Fase 8.
- 2026-11-07: Fase 5: el respaldo es JSON legible (`format: "notable"`, `version`, nodos incluidos los borrados). Importar fusiona sin borrar: el respaldo gana en los ids que comparte y lo local se conserva; si el resultado no es un árbol válido, no se toca nada. Los nodos importados quedan pendientes de sincronizar.
- 2026-11-07: Fase 6: Tauri 2 (2.12) + Leptos 0.8 en modo CSR, compilado con Trunk (puerto 1420); se usan las versiones estables, no las alfas de Tauri 3 / Leptos 0.9. `app/` es la interfaz (`notable-ui`, WebAssembly) y `app/src-tauri/` la app (`notable-app`). Ambas están en el workspace.
- 2026-11-07: Fase 6: crate `notable-api` con los tipos que se intercambian (`Screen`, `Item`, `Crumb`, `Scale`): datos ya listos para mostrar (nota formateada con coma decimal), así la interfaz no calcula nada y no arrastra `uuid` a WebAssembly. La lógica de la app vive en `src-tauri/src/state.rs` sin depender de Tauri (se prueba directo); los comandos son `screen`, `create`, `rename`, `delete`, `restore` y `set_grade`, y cada cambio se guarda al instante en `%APPDATA%\io.github.malvarezdam.notable\notable.db`.
- 2026-11-07: Fase 6: borrar no pide confirmación: muestra "Se borró «X» · Deshacer" durante 8 s (el borrado es lógico). Si la base no abre, la app no toca nada y cada comando muestra el error. Las filas se identifican por id y se actualizan en el lugar (si no, el foco se perdía al pasar de una nota a otra); Enter en una nota salta a la siguiente y los botones ✎/🗑 quedan fuera del orden de Tab.
- 2026-11-07: Fase 6: CSP desactivada (`csp: null`) por ahora: la app no carga nada remoto. Revisar en la Fase 10 (Trunk inyecta un script en línea que una CSP estricta bloquearía). Íconos generados con `cargo tauri icon app/icon.svg` (sin los de iOS).
- 2026-11-07: Fase 7: un solo comando `update(id, Change)` para todos los ajustes (modo, peso, "respecto a", escala, objetivo, redondeo, créditos, reglas); la interfaz manda las reglas completas. `notable-api` trae los datos ya en texto (avisos, efectos de reglas, tarjeta del objetivo, "= 25 % de Tareas").
- 2026-11-07: Fase 7: "Ponderar" es un botón en la barra del nodo; al activarlo por primera vez reparte 100 % en partes iguales (33,33/33,33/33,34) para partir de algo válido. El selector "respecto a" solo aparece con dos o más niveles arriba. Todo lo demás (objetivo, promedio, escala, redondeo, créditos) va en "⚙ Ajustes", y las reglas en un desplegable cerrado dentro de él.
- 2026-11-07: Fase 7: tema Sistema/Claro/Oscuro con un botón (se guarda en `localStorage`, atributo `data-theme`). En pantallas angostas cada fila se parte en dos líneas. Reordenar: al renombrar una fila aparecen ↑/↓ (comando `shift`).
- 2026-11-07: Fase 7: probada en el navegador integrado con un puente HTTP temporal sobre el `state.rs` real (fuera del repo), porque el control de escritorio no alcanza la ventana de desarrollo de Tauri.
- 2026-11-07: Fase 8: proyecto Supabase `augvbftjwvysrozyxvbh` (São Paulo), con "Automatically expose new tables" apagado: `schema.sql` da permisos explícitos solo a `authenticated` y se los quita a `anon`. Confirmación de correo encendida (la app lo explica al crear la cuenta).
- 2026-11-07: Fase 8: crate `notable-sync` con `ureq` 2 (HTTP bloqueante con rustls, sin runtime async) y `time` para fechas RFC 3339. La red está detrás del trait `Remote`; las pruebas usan un servidor en memoria con la misma regla que el trigger.
- 2026-11-07: Fase 8: orden bajar → aplicar → subir. Al bajar se retrocede 60 s desde el último `server_updated_at` visto (por transacciones que terminan tarde) y se pagina por posición (un lote comparte la misma hora). Al aplicar gana el `updated_at` mayor; si el resultado no es un árbol válido no se toca nada. Lo subido se marca limpio solo si no cambió mientras subía.
- 2026-11-07: Fase 8: la sesión (tokens) y el cursor van en la tabla local `meta` (migración 2). Al entrar con una cuenta distinta de la última sincronizada, todo lo local queda pendiente y se fusiona con la cuenta; cerrar sesión no borra nada. Si el refresh falla, se olvida la sesión y se pide volver a entrar.
- 2026-11-07: Fase 8: un hilo sincroniza al abrir, cada 3 min, 4 s después de cada cambio (juntando varios), al pulsar "Sincronizar ahora" y al volver la conexión (evento `online`); sin conexión reintenta cada 30 s. La red nunca se usa con el candado del estado tomado. La interfaz consulta el estado cada 4 s y recarga la pantalla si llegaron cambios de otro dispositivo.
- 2026-11-07: Fase 9: `app/src-tauri/gen/android` se versiona (antes estaba ignorado) porque lleva cambios propios: la firma en `app/build.gradle.kts` y los márgenes del sistema en `MainActivity.kt`. Lo compilado lo excluyen sus `.gitignore`.
- 2026-11-07: Fase 9: clave de firma RSA 4096 (100 años) en `C:\Users\Victus\.android-keys\` con contraseña aleatoria en `notable-keystore.properties`, fuera del repo. Gradle la lee de `NOTABLE_KEYSTORE_PROPERTIES` o de esa ruta; si no existe, el release sale sin firmar.
- 2026-11-07: Fase 9: la app se dibuja de borde a borde (Android 15+), así que `MainActivity` aplica como margen las barras del sistema, el recorte de cámara y el teclado (más confiable que `env(safe-area-inset-*)` en el WebView). En pantallas táctiles los botones son de 40 px y siempre visibles.
- 2026-11-07: Fase 9: se compilan APK separados por ABI: `arm64` (teléfonos) y `x86_64` (emulador). En Windows, `tauri android build` necesita el "Modo para desarrolladores" activado (crea enlaces simbólicos). Sin cable, el APK se pasa al teléfono por Drive/WhatsApp/correo (ver `docs/CONFIGURACION.md` 3.1).
- 2026-11-07: Repo: `malvarezdam/Notable` ya existía con la versión web 1.0 (1 commit, release `v1.0`, GitHub Pages). A pedido del usuario se reemplazó por completo con push forzado; respaldo espejo en `C:\Users\Victus\Desktop\Notable-respaldo-v1.git`.
- 2026-11-07: Fase 10: CI en `ubuntu-24.04` (formato, clippy de todo incluido WebAssembly, pruebas; `app/dist` vacío porque la app incrusta la interfaz). Release por tag `v*`: borrador → Windows y Linux (`cargo tauri build`) y Android (arm64 + armv7, firmado con los secretos `ANDROID_KEYSTORE_BASE64`/`_PASSWORD`/`ANDROID_KEY_ALIAS`) → se publica al final.
- 2026-11-07: Fase 10: archivos del repo: CONTRIBUTING, CODE_OF_CONDUCT (propio, inspirado en Contributor Covenant), SECURITY (reporte privado de GitHub), PRIVACY, CHANGELOG, plantillas de issues y PR; copyright, editor y licencia en los instaladores (`tauri.conf.json`) y `authors` en Cargo.
