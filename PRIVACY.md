# Privacidad

Notable está hecho para que tus notas sean tuyas.

## Sin cuenta (por defecto)

- Todo se guarda **solo en tu dispositivo**: en una base de datos local en la app y en el almacenamiento del navegador en la versión web.
- La app **no envía nada** a internet: ni notas, ni estadísticas de uso, ni publicidad. No hay
  rastreadores.

## Con cuenta (opcional)

Si creas una cuenta para sincronizar entre dispositivos:

- Se guardan en [Supabase](https://supabase.com) (servidor en São Paulo, Brasil): tu **correo**,
  tu contraseña (cifrada por Supabase; la app nunca la guarda) y **lo que escribes en la app**
  (nombres de ramos y evaluaciones, notas, pesos y configuración).
- Esos datos solo los puedes ver tú: las reglas de la base de datos impiden que otro usuario los
  lea. Quien administra el proyecto de Supabase sí tiene acceso técnico a la base; no se usan para
  nada más que sincronizar.
- Supabase puede enviarte correos de la cuenta (por ejemplo, para confirmar el correo).
- La información viaja cifrada (HTTPS).

## Borrar tus datos

- **En el dispositivo:** desinstala la app (o borra sus datos desde los ajustes del sistema). En la versión web, borra los datos del sitio en tu navegador.
- **En la cuenta:** cerrar sesión no borra nada del servidor. Para eliminar tu cuenta y todo lo
  sincronizado, pídelo abriendo un issue o por el reporte privado de [SECURITY.md](SECURITY.md)
  (sin escribir datos personales en público).

## Cambios

Si esto cambia, se avisará en [CHANGELOG.md](CHANGELOG.md) y en las notas de la versión.
