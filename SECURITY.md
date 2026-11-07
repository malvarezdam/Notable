# Seguridad

## Reportar una vulnerabilidad

**No abras un issue público.** Usa el reporte privado de GitHub: pestaña
[Security → Report a vulnerability](https://github.com/malvarezdam/Notable/security/advisories/new).
Indica qué encontraste, cómo reproducirlo y qué podría hacer alguien con eso. Se responderá lo
antes posible y se publicará una corrección antes de dar a conocer el problema.

## Versiones con soporte

Solo la última versión publicada en [Releases](https://github.com/malvarezdam/Notable/releases)
recibe correcciones de seguridad.

## Cómo se protegen los datos

- Las notas se guardan en una base SQLite en el propio dispositivo.
- La sincronización (opcional) usa [Supabase](https://supabase.com) por HTTPS. Cada fila tiene
  dueño y las reglas RLS de [`supabase/schema.sql`](supabase/schema.sql) impiden que un usuario
  lea o modifique datos de otro; sin sesión iniciada no se puede acceder a la tabla.
- La URL del proyecto y la clave *publishable* que están en el código son **públicas a
  propósito** (van dentro de la app que cualquiera descarga); no son una vulnerabilidad. La clave
  secreta de Supabase y la contraseña de la base de datos nunca están en el repositorio.
- Las contraseñas de las cuentas las maneja Supabase Auth; la app no las guarda, solo la sesión
  (tokens) en la base local del dispositivo.
- Los APK se firman con una clave guardada fuera del repositorio (secretos de GitHub Actions).
