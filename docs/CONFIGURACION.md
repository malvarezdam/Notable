# Configuración que debes hacer tú

Estos son los pasos que hace el dueño del proyecto (tú), porque requieren tus cuentas.
Todo es gratis. **No necesitas hacerlos todos ahora**: cada parte dice en qué fase del
[PLAN.md](../PLAN.md) hace falta.

---

## Parte 1 — Antes de la Fase 0 (ahora)

### 1.1 Sacar el proyecto de OneDrive (muy recomendado)
La carpeta actual está dentro de OneDrive. Rust genera miles de archivos en `target/` (varios GB)
y OneDrive intentará subirlos, bloqueará archivos durante la compilación y llenará tu espacio.
Con Git + GitHub ya tienes respaldo, así que no necesitas OneDrive para esto.

1. Crea una carpeta fuera de OneDrive, por ejemplo `C:\dev\Notable`.
2. Mueve ahí `PLAN.md`, `CLAUDE.md`, `docs/` y `supabase/` (la carpeta `target/` puedes borrarla).
3. Abre una sesión nueva de Claude en esa carpeta.

### 1.2 Herramientas (ya revisadas)
Ya tienes instalados Rust 1.97, Git, GitHub CLI (`gh`), Java 22 y Node 24. Solo confirma que tu
sesión de `gh` funciona:

```bash
gh auth status
```

Si dice que no has iniciado sesión: `gh auth login` y sigue las instrucciones (GitHub.com → HTTPS → navegador).

---

## Parte 2 — Supabase (antes de la Fase 8, pero puedes hacerlo cuando quieras)

### 2.1 Crear la cuenta y el proyecto
1. Entra a <https://supabase.com> → **Start your project** → inicia sesión **con GitHub** (gratis, sin tarjeta).
2. **New project**:
   - **Organization:** la personal que te crea (plan *Free*).
   - **Name:** `notable`
   - **Database Password:** pulsa *Generate a password* y **guárdala en tu gestor de contraseñas**.
     No la compartas con nadie, tampoco con Claude: la app no la necesita.
   - **Region:** *South America (São Paulo)*, la más cercana a Chile.
3. **Create new project** y espera 1–2 minutos.

### 2.2 Crear la tabla
1. En el menú izquierdo: **SQL Editor** → **New query**.
2. Abre el archivo [`supabase/schema.sql`](../supabase/schema.sql), copia **todo** su contenido y pégalo.
3. **Run**. Debe decir *Success. No rows returned*.
4. Comprueba en **Table Editor** que existe la tabla `items` y que dice **RLS enabled**.

### 2.3 Activar el inicio de sesión con correo
1. **Authentication** → **Sign In / Providers** → **Email**: debe estar **activado** (lo está por defecto).
2. **Confirm email**:
   - Activado (por defecto): cada cuenta nueva debe confirmar su correo. El correo gratis de Supabase
     tiene un límite muy bajo (unos pocos por hora); para uso personal alcanza.
   - Si te molesta, puedes desactivarlo. Para una app personal es aceptable.
3. **Authentication** → **URL Configuration** → **Site URL**: pon `https://github.com/malvarezdam/Notable/releases/latest`
   (por defecto es `localhost:3000` y el link de confirmación termina en un error).
4. **Recuperar contraseña:** la app pide un código, así que en **Authentication** → **Emails** →
   **Reset password** el mensaje debe mostrarlo. Deja el cuerpo así:
   ```html
   <h2>Recuperar tu contraseña de Notable</h2>
   <p>Escribe este código en la app: <strong>{{ .Token }}</strong></p>
   ```

### 2.4 Datos para la app
1. Ve a **Project Settings** → **API Keys** (o **Data API**, según la versión del panel).
2. Copia estos dos valores:
   - **Project URL** (algo como `https://abcdxyz.supabase.co`)
   - **Publishable key** (empieza con `sb_publishable_…`) o, si tu panel la muestra así, la **anon public key**.
3. Pásaselos a Claude cuando lleguemos a la Fase 8. **Pueden ir en el repositorio público**: la
   seguridad la dan las reglas RLS de la tabla, no el secreto de estas claves.

> ⚠️ **Nunca** copies ni compartas la **secret key** / **service_role key** ni la contraseña de la
> base de datos. Esas sí dan acceso total y no deben aparecer en el código.

### 2.5 Lo que debes saber del plan gratis
- Si nadie usa el proyecto durante **7 días**, Supabase lo **pausa**. Tus datos no se pierden.
  Para reactivarlo: entra a supabase.com → proyecto `notable` → **Restore project**.
  Mientras está pausado, la app sigue funcionando con lo guardado en el dispositivo y sincroniza
  cuando lo reactives.
- Límites (aprox.): 500 MB de base de datos y 50.000 usuarios activos al mes. Las notas de una
  persona ocupan pocos KB.

---

## Parte 3 — Android (antes de la Fase 9)

1. Instala **Android Studio**: <https://developer.android.com/studio>.
2. Abre Android Studio → **More Actions** → **SDK Manager**:
   - Pestaña **SDK Platforms**: marca la versión más reciente de Android.
   - Pestaña **SDK Tools**: marca **Android SDK Build-Tools**, **NDK (Side by side)**,
     **Android SDK Command-line Tools** y **Android SDK Platform-Tools** → **Apply**.
3. Variables de entorno (Inicio → "Editar las variables de entorno de esta cuenta"):
   - `ANDROID_HOME` = `C:\Users\Victus\AppData\Local\Android\Sdk`
   - `NDK_HOME` = `%ANDROID_HOME%\ndk\<la versión instalada>` (mira el nombre de la carpeta)
   - `JAVA_HOME` = `C:\Program Files\Android\Android Studio\jbr`
4. Cierra y vuelve a abrir la terminal (y Claude) para que tomen las variables.
5. Los objetivos de Rust para Android los instala Claude en la Fase 9.
6. (Solo si vas a conectar el teléfono por cable) En tu teléfono: **Ajustes → Acerca del
   teléfono** → toca 7 veces "Número de compilación" → en **Opciones de desarrollador** activa
   **Depuración USB**.

### 3.1 Instalar el APK sin cable
El APK firmado queda en
`app/src-tauri/gen/android/app/build/outputs/apk/arm64/release/app-arm64-release.apk`
(la mayoría de los teléfonos son arm64).
1. Pásalo al teléfono como te acomode: Google Drive, WhatsApp a ti mismo, correo o pendrive.
2. En el teléfono, ábrelo. Android pedirá permitir "instalar apps desconocidas" para la app desde
   la que lo abres (Drive, WhatsApp…): permítelo solo para eso.
3. Para actualizar, instala el APK nuevo encima: se conservan tus notas.

### 3.2 La clave de firma (¡respáldala!)
Claude la generó en `C:\Users\Victus\.android-keys\`:
- `notable-release.jks`: la clave.
- `notable-keystore.properties`: su contraseña y alias.

**Respalda esa carpeta completa** (gestor de contraseñas, pendrive, nube personal). Android solo
acepta actualizaciones firmadas con la misma clave: si se pierde, habría que desinstalar la app
(perdiendo lo que no esté sincronizado) para instalar una versión nueva. Nunca la subas al
repositorio. Gradle la lee de esa carpeta, o del archivo que diga la variable de entorno
`NOTABLE_KEYSTORE_PROPERTIES` (así la usará el workflow de la Fase 10).

---

## Parte 4 — Publicar (Fase 10)
Los workflows ya están en `.github/workflows/`:
- `ci.yml`: formato, clippy y pruebas en cada push a `main`.
- `release.yml`: al subir un tag `v*` arma el `.exe` portable de Windows (sin instalador) y los
  `.apk` de Android, y los publica en **Releases**.

### 4.1 Secretos para firmar el APK
En GitHub → repo `Notable` → **Settings → Secrets and variables → Actions → New repository
secret**, crea estos tres (sin ellos el APK sale sin firmar y no se puede instalar):

| Nombre | Valor |
|---|---|
| `ANDROID_KEYSTORE_BASE64` | el archivo `C:\Users\Victus\.android-keys\notable-release.jks` en base64 (ver abajo) |
| `ANDROID_KEYSTORE_PASSWORD` | el valor de `storePassword` en `notable-keystore.properties` |
| `ANDROID_KEY_ALIAS` | `notable` |

Para sacar el base64 (copia el resultado al portapapeles), en PowerShell:
```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("$env:USERPROFILE\.android-keys\notable-release.jks")) | Set-Clipboard
```

### 4.2 Publicar una versión
1. Sube el número de versión en `app/src-tauri/tauri.conf.json` (`"version"`) y en `Cargo.toml`
   (`[workspace.package] version`), y haz commit.
2. Crea y sube el tag: `git tag v1.1.0` y `git push origin v1.1.0`.
3. En la pestaña **Actions** se ve el avance (~20 min). Al terminar, la versión aparece en
   **Releases** con todos los archivos.
