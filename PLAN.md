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
- [ ] Crear repo en GitHub con `gh` (lo confirma el usuario) y primer push.
**Listo cuando:** `cargo test` pasa en el workspace y el repo existe en GitHub.

### Fase 1 — Modelo de datos (`notable-core`)
- [ ] Tipos: `Node`, `NodeId` (UUID), `GradeConfig` (escala, aprobación, decimales, `Rounding::{Round, Truncate}`), `Aggregation::{Simple, Weighted}`.
- [ ] Árbol en memoria con operaciones: crear, mover, renombrar, borrar (borrado lógico), editar nota.
- [ ] Herencia de `GradeConfig` (un nodo usa la de su ancestro más cercano que la defina).
- [ ] Serialización serde (JSON) de cada nodo por separado (sirve para SQLite y sync).
**Listo cuando:** hay pruebas de construcción del árbol, herencia y serialización ida y vuelta.

### Fase 2 — Motor de cálculo
- [ ] Promedio de cada nodo: simple o ponderado, ignorando hijos sin nota (nota "parcial actual").
- [ ] Pesos expresados respecto a un ancestro ("vale 20% del curso") → conversión a peso relativo al padre.
- [ ] Validaciones como avisos (no errores): pesos que no suman 100 %, nota fuera de escala, pesos contradictorios.
- [ ] Redondeo/truncado configurable: solo en la nota final del nodo con objetivo, o también en cada nivel (opción).
- [ ] Conversión entre escalas cuando un hijo usa una escala distinta a la del padre (lineal).
**Listo cuando:** pruebas con casos reales (ramo USM 0–100, ramo 1–7, mezcla de escalas, redondeo intermedio).

### Fase 3 — Nota objetivo y nota mínima necesaria
- [ ] Objetivo opcional por nodo (normalmente el ramo).
- [ ] Si falta exactamente 1 nota: nota mínima necesaria. Despeje lineal cuando no hay redondeo intermedio;
      búsqueda sobre los valores posibles de la escala (según decimales) cuando sí lo hay.
- [ ] Resultados: `Necesitas(x)`, `YaCumplido`, `NoAlcanzable`.
- [ ] Extra: si faltan varias notas, promedio necesario en lo que falta (asumiendo la misma nota en todas).
**Listo cuando:** pruebas de casos borde (exacto en el límite, truncado vs aproximado, ya cumplido, imposible).

### Fase 4 — Reglas opcionales (todas apagadas por defecto)
- [ ] Eliminar la(s) peor(es) nota(s) de un nodo.
- [ ] Examen/nota que reemplaza a la peor nota (o a una nota elegida).
- [ ] Eximición: si el promedio ≥ X, una evaluación (p. ej. examen) no se rinde/no cuenta.
- [ ] Requisito mínimo: "este nodo debe tener ≥ X para aprobar el padre" (muestra aviso/estado reprobado).
- [ ] Promedio de semestre/año/carrera ponderado por créditos (campo "créditos" opcional en el ramo).
- [ ] Integrar las reglas con la Fase 3 (la nota mínima las respeta).
**Listo cuando:** cada regla tiene pruebas encendida y apagada.

### Fase 5 — Guardado local (`notable-store`)
- [ ] SQLite con `rusqlite` (feature `bundled`): tabla `items(id, parent_id, data JSON, updated_at, deleted, dirty)`.
- [ ] Migraciones simples por versión.
- [ ] Cargar el árbol completo al iniciar; guardar cada cambio al instante (`dirty = 1`).
- [ ] Exportar/importar a archivo JSON (respaldo manual, sin cuenta).
**Listo cuando:** pruebas de guardar → cerrar → abrir dan el mismo árbol.

### Fase 6 — App base (Tauri 2 + Leptos)
- [ ] Proyecto Tauri 2 en `app/` usando los crates; comandos Tauri para leer/modificar el árbol.
- [ ] Pantalla principal: navegación por el árbol (lista + migas de pan), crear/renombrar/borrar nodos, ingresar notas.
- [ ] Cada nodo muestra su promedio actual.
**Listo cuando:** en Windows se puede armar un ramo completo e ingresar notas, y se guarda al cerrar.

### Fase 7 — UI completa
- [ ] Botón "Ponderar" visible en cada nodo con hijos; edición de pesos (y "respecto a" otro nivel).
- [ ] Panel de configuración de nota (escala, decimales, truncar/aproximar), heredable.
- [ ] Objetivo y tarjeta "Necesitas X en …".
- [ ] Panel "Reglas opcionales" (Fase 4), plegado por defecto.
- [ ] Diseño adaptable a teléfono, tema claro/oscuro.
**Listo cuando:** se pueden usar todas las funciones de Fases 1–4 desde la interfaz.

### Fase 8 — Cuenta y sincronización (`notable-sync`)
- [ ] Requiere que el usuario haya hecho `docs/CONFIGURACION.md` (proyecto Supabase + `schema.sql`).
- [ ] Login/registro con correo y contraseña (Supabase Auth REST), guardar la sesión y refrescar el token.
- [ ] Subir los `dirty` (upsert), bajar cambios con `server_updated_at` mayor al último sync; gana el `updated_at` más reciente.
- [ ] Sincronizar al abrir, al volver a tener internet y cada pocos minutos; indicador de estado.
- [ ] Primer login en un dispositivo con datos locales: fusionar (no borrar nada).
- [ ] Manejar proyecto pausado/sin conexión sin romper la app.
**Listo cuando:** cambios hechos en PC aparecen en otra instalación y viceversa, también tras trabajar sin conexión.

### Fase 9 — Android
- [ ] Requiere herramientas Android del usuario (ver `docs/CONFIGURACION.md`, parte 3).
- [ ] `tauri android init`, ajustar UI táctil, compilar APK firmado (clave guardada fuera del repo).
**Listo cuando:** el APK se instala en el teléfono del usuario y sincroniza con el PC.

### Fase 10 — CI y publicación
- [ ] GitHub Actions: `cargo test` + clippy en cada push.
- [ ] Workflow de release (al crear un tag `v*`): instalador Windows, AppImage/deb Linux, APK Android → GitHub Releases.
- [ ] Secretos del workflow (clave de firma Android) como GitHub Secrets.
- [ ] README con capturas, instalación y cómo usar.
**Listo cuando:** crear un tag publica una release descargable.

---

## Registro de decisiones
- 2026-11-07: Local primero (SQLite) + sincronización opcional con Supabase gratis; Tauri 2 + Leptos; distribución por GitHub Releases; iOS fuera por ahora.
- 2026-11-07: Promedio simple por defecto + botón "Ponderar". Todas las reglas extra son opcionales y configurables.
- 2026-11-07: El esquema remoto es genérico (`items` con `data jsonb`), así los cambios del modelo no obligan a tocar Supabase.
- 2026-11-07: Fase 0: workspace con edición 2024 y `resolver = "3"`; `Cargo.lock` se versiona (habrá binario). Repo: `malvarezdam/notable`, rama `main`.
