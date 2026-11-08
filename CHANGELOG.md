# Cambios

Todos los cambios importantes de Notable. El formato sigue
[Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y las versiones siguen
[versionado semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Cambiado
- Descargas: Windows pasa a ser un `.exe` portable (sin instalador) y se dejan de publicar los
  paquetes de Linux. Se mantienen los APK de Android.

## [1.0.0] - 2026-11-07

Primera versión de la app nativa en Rust (reemplaza a la versión web anterior).

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
- Instalador para Windows, paquetes para Linux y APK para Android.

[Sin publicar]: https://github.com/malvarezdam/Notable/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/malvarezdam/Notable/releases/tag/v1.0.0
