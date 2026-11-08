# Cambios

Todos los cambios importantes de Notable. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y las versiones siguen
[versionado semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Agregado
- **Cambiar contraseña** desde el panel de cuenta (pide la contraseña actual).
- **Recuperar una contraseña olvidada** con un código enviado por correo, antes de entrar.

## [1.0.0] - 2026-11-07

Primera versión de la app nativa en Rust (reemplaza a la versión web anterior). Los archivos de
la release se actualizaron el 2026-11-08 con lo marcado como *(08-11)*.

### Agregado
- Árbol libre de carreras, años, ramos, secciones y evaluaciones, con migas de pan.
- Promedio simple por defecto y botón **Ponderar** con pesos en %, también "respecto a" otro nivel.
- Escalas configurables y heredables (1,0–7,0, 0–100 o propias), decimales, aproximar o truncar,
  redondeo por nivel y conversión entre escalas (lineal o por nota de aprobación).
- Nota objetivo y cálculo de la nota mínima necesaria ("Necesitas X en …").
- Reglas opcionales: eliminar las peores notas, nota que reemplaza a otra, eximición, nota mínima
  para aprobar y promedio por créditos.
- Avisos (pesos que no suman 100 %, notas fuera de escala, etc.) sin bloquear el cálculo.
- Guardado local en SQLite al instante.
- Respaldo en archivo: **Exportar** e **Importar** (JSON) desde el panel ☁, sin cuenta; importar
  agrega sin borrar lo que ya hay.
- Cuenta opcional y sincronización entre dispositivos con Supabase (gana el cambio más reciente,
  funciona sin conexión).
- Tema claro/oscuro y diseño para teléfono.
- *(08-11)* **Mover** una sección o evaluación a otro lugar (botón ⇄), con todo lo que tiene
  adentro, sin tener que borrarla.
- *(08-11)* Regla opcional **factor**: una nota (por ejemplo, el laboratorio) que no se promedia
  sino que multiplica la nota de arriba por un factor dentro de un rango (puede subirla o
  bajarla), con una nota opcional que deja el factor en 1.
- `.exe` portable para Windows (sin instalador) y APK para Android.

[Sin publicar]: https://github.com/malvarezdam/Notable/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/malvarezdam/Notable/releases/tag/v1.0.0
