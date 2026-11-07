//! Interfaz de Notable. No calcula nada: le pide a la app (comandos Tauri) la pantalla ya lista
//! y le avisa de cada cambio; después de cada cambio vuelve a pedir la pantalla.

use std::time::Duration;

use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use notable_api::{
    Account, Change, Details, ExemptionSettings, GoalKind, MinimumSettings, Mode, ReplaceSettings,
    RulesSettings, Scale, ScaleSettings, Screen, SyncStatus, format_grade, format_number,
    parse_grade,
};
use serde::Serialize;
use serde::de::{DeserializeOwned, IgnoredAny};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

// ------------------------------------------------------------------ comandos

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

async fn call<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, String> {
    let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    let value = invoke(cmd, args)
        .await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{e:?}")))?;
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct ViewArgs {
    view: Option<String>,
}

#[derive(Serialize)]
struct IdArgs {
    id: String,
}

#[derive(Serialize)]
struct CreateArgs {
    parent: Option<String>,
    name: String,
}

#[derive(Serialize)]
struct RenameArgs {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct GradeArgs {
    id: String,
    grade: Option<f64>,
}

#[derive(Serialize)]
struct ShiftArgs {
    id: String,
    delta: i32,
}

#[derive(Serialize)]
struct NoArgs {}

#[derive(Serialize)]
struct LoginArgs {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct UpdateArgs {
    id: String,
    change: Change,
}

// ------------------------------------------------------------------ estado

#[derive(Clone, PartialEq)]
enum Toast {
    Error(String),
    Deleted { id: String, name: String },
}

/// Estado de la interfaz, compartido por contexto.
#[derive(Clone, Copy)]
struct Ctx {
    /// Nodo que se está viendo (`None` = inicio).
    view: RwSignal<Option<String>>,
    screen: RwSignal<Screen>,
    toast: RwSignal<Option<Toast>>,
    /// Panel de ajustes del nodo actual abierto.
    settings: RwSignal<bool>,
    /// Estado de la cuenta y la sincronización.
    account: RwSignal<Account>,
    /// Panel de cuenta abierto.
    account_open: RwSignal<bool>,
}

impl Ctx {
    /// Vuelve a pedir la pantalla del nodo actual.
    fn reload(self) {
        spawn_local(async move {
            let view = self.view.get_untracked();
            match call::<_, Screen>("screen", &ViewArgs { view: view.clone() }).await {
                Ok(screen) => {
                    // Si el nodo ya no existe, la app muestra un ancestro.
                    let shown = screen.current.as_ref().map(|c| c.id.clone());
                    if shown != view {
                        self.view.set(shown);
                    }
                    self.screen.set(screen);
                }
                Err(e) => self.error(e),
            }
        });
    }

    fn go(self, view: Option<String>) {
        if view != self.view.get_untracked() {
            self.settings.set(false);
        }
        self.view.set(view);
        self.reload();
    }

    fn error(self, message: String) {
        self.toast.set(Some(Toast::Error(message)));
    }

    /// Ejecuta un comando que cambia algo; si sale bien llama a `then`. Siempre recarga.
    fn run<A: Serialize + 'static>(
        self,
        cmd: &'static str,
        args: A,
        then: impl FnOnce(Self) + 'static,
    ) {
        spawn_local(async move {
            match call::<_, IgnoredAny>(cmd, &args).await {
                Ok(_) => then(self),
                Err(e) => self.error(e),
            }
            self.reload();
        });
    }

    /// Cambia un ajuste de un nodo.
    fn update(self, id: String, change: Change) {
        self.run("update", UpdateArgs { id, change }, |_| {});
    }

    /// Cambia un ajuste del nodo actual.
    fn update_current(self, change: Change) {
        if let Some(id) = self.current_id() {
            self.update(id, change);
        }
    }

    fn current_id(self) -> Option<String> {
        self.screen
            .with_untracked(|s| s.current.as_ref().map(|c| c.id.clone()))
    }

    /// Pide el estado de la cuenta; si llegaron cambios de otro dispositivo, recarga la pantalla.
    fn poll_account(self) {
        spawn_local(async move {
            if let Ok(account) = call::<_, Account>("account", &NoArgs {}).await {
                let changed = account.revision != self.account.get_untracked().revision;
                if self.account.get_untracked() != account {
                    self.account.set(account);
                }
                if changed {
                    self.reload();
                }
            }
        });
    }
}

// ------------------------------------------------------------------ tema

const THEMES: [(&str, &str); 3] = [
    ("", "Tema: sistema"),
    ("light", "Tema: claro"),
    ("dark", "Tema: oscuro"),
];

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

fn apply_theme(theme: &str) {
    if let Some(root) = document().document_element() {
        let _ = if theme.is_empty() {
            root.remove_attribute("data-theme")
        } else {
            root.set_attribute("data-theme", theme)
        };
    }
}

// ------------------------------------------------------------------ vistas

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    let ctx = Ctx {
        view: RwSignal::new(None),
        screen: RwSignal::new(Screen::default()),
        toast: RwSignal::new(None),
        settings: RwSignal::new(false),
        account: RwSignal::new(Account::default()),
        account_open: RwSignal::new(false),
    };
    provide_context(ctx);
    ctx.reload();
    ctx.poll_account();
    // El estado de la sincronización se consulta seguido (es local y barato).
    set_interval(move || ctx.poll_account(), Duration::from_secs(4));
    // Al volver la conexión, sincronizar al tiro.
    let _online = window_event_listener(leptos::ev::online, move |_| {
        ctx.run("sync_now", NoArgs {}, |ctx| ctx.poll_account());
    });

    view! {
        <main>
            <header>
                <Crumbs />
                <SyncButton />
                <ThemeButton />
            </header>
            <Show when=move || ctx.account_open.get()>
                <AccountPanel />
            </Show>
            <Summary />
            <Show when=move || ctx.settings.get()>
                <SettingsPanel />
            </Show>
            <Children />
            <AddForm />
        </main>
        <ToastView />
    }
}

/// Indicador de sincronización; abre el panel de cuenta.
#[component]
fn SyncButton() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let look = move || {
        ctx.account.with(|a| match a.status {
            SyncStatus::SignedOut => ("☁", "Cuenta", "sync off"),
            SyncStatus::Syncing => ("⟳", "Sincronizando", "sync busy"),
            SyncStatus::Synced if a.pending > 0 => ("☁", "Pendiente", "sync busy"),
            SyncStatus::Synced => ("☁", "Al día", "sync ok"),
            SyncStatus::Offline => ("☁", "Sin conexión", "sync warn"),
            SyncStatus::Error => ("⚠", "Error", "sync warn"),
        })
    };
    view! {
        <button
            class=move || format!("chip {}", look().2)
            class:on=move || ctx.account_open.get()
            title=move || ctx.account.with(|a| a.message.clone())
            on:click=move |_| ctx.account_open.update(|o| *o = !*o)
        >
            <span class="sync-icon">{move || look().0}</span>
            <span class="sync-text">{move || look().1}</span>
        </button>
    }
}

/// Panel de cuenta: entrar, crear cuenta, estado y cerrar sesión. La cuenta es opcional.
#[component]
fn AccountPanel() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);

    let login = move |sign_up: bool| {
        let args = LoginArgs {
            email: email.get_untracked(),
            password: password.get_untracked(),
        };
        if args.email.trim().is_empty() || args.password.is_empty() {
            note.set(Some("Escribe tu correo y una contraseña.".into()));
            return;
        }
        busy.set(true);
        note.set(None);
        spawn_local(async move {
            let result = if sign_up {
                call::<_, String>("sign_up", &args).await.map(Some)
            } else {
                call::<_, IgnoredAny>("sign_in", &args).await.map(|_| None)
            };
            busy.set(false);
            match result {
                Ok(message) => {
                    password.set(String::new());
                    note.set(message);
                }
                Err(e) => note.set(Some(e)),
            }
            ctx.poll_account();
        });
    };
    let sign_out = move |_| {
        spawn_local(async move {
            if let Err(e) = call::<_, IgnoredAny>("sign_out", &NoArgs {}).await {
                ctx.error(e);
            }
            note.set(None);
            ctx.poll_account();
        });
    };
    let sync_now = move |_| ctx.run("sync_now", NoArgs {}, |ctx| ctx.poll_account());
    // Respaldo en archivo (sin cuenta). `None` = el usuario canceló el diálogo.
    let backup = move |cmd: &'static str| {
        busy.set(true);
        note.set(None);
        spawn_local(async move {
            match call::<_, Option<String>>(cmd, &NoArgs {}).await {
                Ok(message) => note.set(message),
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
            ctx.reload();
            ctx.poll_account();
        });
    };

    let signed_in = move || ctx.account.with(|a| a.email.is_some());
    let pending = move || {
        ctx.account.with(|a| match a.pending {
            0 => String::new(),
            1 => "1 cambio por subir.".into(),
            n => format!("{n} cambios por subir."),
        })
    };

    view! {
        <section class="panel account">
            <Show
                when=signed_in
                fallback=move || {
                    view! {
                        <p class="hint">
                            "Opcional: con una cuenta tus notas se sincronizan entre el PC y el teléfono. "
                            "Sin cuenta todo funciona igual, solo en este dispositivo."
                        </p>
                        <div class="login">
                            <input
                                class="text"
                                type="email"
                                placeholder="Correo"
                                autocomplete="email"
                                prop:value=move || email.get()
                                on:input=move |ev| email.set(event_target_value(&ev))
                            />
                            <input
                                class="text"
                                type="password"
                                placeholder="Contraseña"
                                autocomplete="current-password"
                                prop:value=move || password.get()
                                on:input=move |ev| password.set(event_target_value(&ev))
                                on:keydown=move |ev: KeyboardEvent| {
                                    if ev.key() == "Enter" {
                                        login(false)
                                    }
                                }
                            />
                            <div class="buttons">
                                <button class="primary" disabled=move || busy.get() on:click=move |_| login(false)>
                                    "Entrar"
                                </button>
                                <button class="chip" disabled=move || busy.get() on:click=move |_| login(true)>
                                    "Crear cuenta"
                                </button>
                            </div>
                        </div>
                        <p class="hint">{move || ctx.account.with(|a| a.message.clone())}</p>
                    }
                }
            >
                <div class="field">
                    <span class="label">{move || ctx.account.with(|a| a.email.clone().unwrap_or_default())}</span>
                    <span class="hint">{move || ctx.account.with(|a| a.message.clone())} " " {pending}</span>
                </div>
                <div class="buttons">
                    <button class="chip" on:click=sync_now>"Sincronizar ahora"</button>
                    <button class="chip" on:click=sign_out>"Cerrar sesión"</button>
                </div>
                <p class="hint">"Al cerrar sesión tus notas se quedan en este dispositivo."</p>
            </Show>
            <div class="field backup">
                <span class="label">"Respaldo en archivo"</span>
                <span class="hint">
                    "Guarda todo en un archivo para tenerlo a salvo o pasarlo a otro dispositivo. "
                    "Importar agrega lo del archivo sin borrar lo que ya tienes."
                </span>
                <div class="buttons">
                    <button class="chip" disabled=move || busy.get() on:click=move |_| backup("export_backup")>
                        "Exportar respaldo"
                    </button>
                    <button class="chip" disabled=move || busy.get() on:click=move |_| backup("import_backup")>
                        "Importar respaldo"
                    </button>
                </div>
            </div>
            {move || note.get().map(|n| view! { <p class="note">{n}</p> })}
        </section>
    }
}

#[component]
fn ThemeButton() -> impl IntoView {
    let saved = storage()
        .and_then(|s| s.get_item("theme").ok().flatten())
        .unwrap_or_default();
    let index = RwSignal::new(THEMES.iter().position(|(t, _)| *t == saved).unwrap_or(0));
    apply_theme(THEMES[index.get_untracked()].0);
    let next = move |_| {
        let i = (index.get_untracked() + 1) % THEMES.len();
        index.set(i);
        let theme = THEMES[i].0;
        apply_theme(theme);
        if let Some(s) = storage() {
            let _ = s.set_item("theme", theme);
        }
    };
    let icon = move || match THEMES[index.get()].0 {
        "light" => "☀",
        "dark" => "☾",
        _ => "◐",
    };
    view! {
        <button class="icon theme" title=move || THEMES[index.get()].1 on:click=next>
            {icon}
        </button>
    }
}

/// Migas de pan: Inicio › Carrera › Ramo…
#[component]
fn Crumbs() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    view! {
        <nav class="crumbs">
            <button
                class:here=move || ctx.screen.with(|s| s.path.is_empty())
                on:click=move |_| ctx.go(None)
            >
                "Inicio"
            </button>
            {move || {
                let path = ctx.screen.with(|s| s.path.clone());
                let last = path.len().saturating_sub(1);
                path.into_iter()
                    .enumerate()
                    .map(|(i, crumb)| {
                        let id = crumb.id.clone();
                        view! {
                            <span>"›"</span>
                            <button class:here={i == last} on:click=move |_| ctx.go(Some(id.clone()))>
                                {crumb.name}
                            </button>
                        }
                    })
                    .collect_view()
            }}
        </nav>
    }
}

/// Tarjeta con el nombre y la nota del nodo actual, la barra de acciones, el objetivo y los
/// avisos.
#[component]
fn Summary() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let details = Memo::new(move |_| ctx.screen.with(|s| s.details.clone()));
    move || {
        ctx.screen.with(|s| s.current.clone()).map(|item| {
            let state = if item.complete {
                "Nota final"
            } else if item.value.is_some() {
                "Nota parcial, con lo ingresado"
            } else {
                "Sin notas todavía"
            };
            let class = match item.passed {
                Some(true) => "big pass",
                Some(false) => "big fail",
                None => "big",
            };
            let has_children = !item.is_leaf;
            let weighted =
                move || details.with(|d| d.as_ref().is_some_and(|d| d.mode == Mode::Weighted));
            let toggle_weighting = move |_| {
                let mode = if weighted() {
                    Mode::Simple
                } else {
                    Mode::Weighted
                };
                ctx.update_current(Change::Mode(mode));
            };
            let credits =
                move || details.with(|d| d.as_ref().is_some_and(|d| d.mode == Mode::Credits));
            view! {
                <section class="summary">
                    <div class="title">
                        <h1>{item.name}</h1>
                        <div class="state">{state}</div>
                    </div>
                    <div class=class>{item.display}</div>
                </section>
                <div class="toolbar">
                    <Show when=move || has_children>
                        <button
                            class="chip"
                            class:on=weighted
                            title="Usar pesos en vez de promedio simple"
                            on:click=toggle_weighting
                        >
                            {move || if weighted() { "Ponderado ✓" } else { "Ponderar" }}
                        </button>
                        <Show when=credits>
                            <span class="chip on static">"Por créditos"</span>
                        </Show>
                    </Show>
                    <button
                        class="chip"
                        class:on=move || ctx.settings.get()
                        on:click=move |_| ctx.settings.update(|o| *o = !*o)
                    >
                        "⚙ Ajustes"
                    </button>
                </div>
                <GoalCard />
                {move || {
                    details
                        .with(|d| d.as_ref().map(|d| d.warnings.clone()).unwrap_or_default())
                        .into_iter()
                        .map(|w| view! { <p class="warning">"⚠ " {w}</p> })
                        .collect_view()
                }}
            }
        })
    }
}

/// "Necesitas X en …" cuando el nodo tiene objetivo.
#[component]
fn GoalCard() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    move || {
        ctx.screen
            .with(|s| s.details.as_ref().and_then(|d| d.goal.clone()))
            .map(|goal| {
                let class = match goal.kind {
                    GoalKind::Needed => "goal",
                    GoalKind::Met => "goal met",
                    GoalKind::Unreachable => "goal unreachable",
                };
                view! {
                    <section class=class>
                        <strong>{goal.title}</strong>
                        <span>{goal.detail}</span>
                    </section>
                }
            })
    }
}

#[component]
fn Children() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let empty_text = move || {
        if ctx.screen.with(|s| s.path.is_empty()) {
            "Agrega lo que quieras organizar: una carrera, un año o un ramo."
        } else {
            "Todavía no hay nada aquí. Agrega evaluaciones o secciones abajo."
        }
    };
    // Las filas se identifican solo por id: al cambiar una nota la fila se actualiza en el lugar
    // (no se vuelve a crear), así no se pierde el foco al pasar de una nota a la siguiente.
    let ids = move || {
        ctx.screen
            .with(|s| s.children.iter().map(|i| i.id.clone()).collect::<Vec<_>>())
    };
    view! {
        <Show
            when=move || ctx.screen.with(|s| !s.children.is_empty())
            fallback=move || view! { <p class="empty">{empty_text}</p> }
        >
            <ul class="list">
                <For each=ids key=|id| id.clone() children=|id| view! { <Row id /> } />
            </ul>
        </Show>
    }
}

/// Texto con que se muestra una nota ingresada para editarla.
fn input_text(grade: f64, scale: Scale) -> String {
    let formatted = format_grade(grade, scale.decimals);
    if parse_grade(&formatted) == Ok(Some(grade)) {
        formatted
    } else {
        grade.to_string().replace('.', ",")
    }
}

/// Pasa el foco al siguiente campo de nota (Enter en una nota).
fn focus_next_grade(current: &web_sys::EventTarget) {
    let Ok(list) = document().query_selector_all("input.grade-input") else {
        return;
    };
    let current = current.dyn_ref::<web_sys::Node>();
    let mut found = false;
    for i in 0..list.length() {
        let Some(node) = list.item(i) else { continue };
        if found {
            if let Ok(input) = node.dyn_into::<web_sys::HtmlElement>() {
                let _ = input.focus();
            }
            return;
        }
        found = node.is_same_node(current);
    }
    // Era la última: salir del campo para que se guarde.
    if let Some(input) = current.and_then(|n| n.dyn_ref::<web_sys::HtmlElement>()) {
        let _ = input.blur();
    }
}

/// Campo de número (nota, peso, créditos). Guarda al salir del campo o con Enter; vacío = sin
/// valor.
#[component]
fn NumberField(
    #[prop(into)] value: Signal<Option<f64>>,
    on_set: Callback<Option<f64>>,
    #[prop(optional)] placeholder: &'static str,
    #[prop(optional)] class: &'static str,
    #[prop(optional)] title: &'static str,
) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let text = RwSignal::new(String::new());
    Effect::new(move |_| {
        let v = value.get();
        if parse_grade(&text.get_untracked()) != Ok(v) {
            text.set(v.map(format_number).unwrap_or_default());
        }
    });
    let invalid = move || parse_grade(&text.get()).is_err();
    view! {
        <input
            class=format!("number {class}")
            class:invalid=invalid
            inputmode="decimal"
            placeholder=placeholder
            title=title
            prop:value=move || text.get()
            on:input=move |ev| text.set(event_target_value(&ev))
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Enter"
                    && let Some(input) = ev
                        .target()
                        .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    let _ = input.blur();
                }
            }
            on:change=move |_| match parse_grade(&text.get_untracked()) {
                Ok(new) if new != value.get_untracked() => on_set.run(new),
                Ok(_) => {}
                Err(e) => ctx.error(e),
            }
        />
    }
}

#[component]
fn Row(id: String) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    // Los datos de la fila salen de la pantalla actual y se actualizan solos.
    let item = Memo::new({
        let id = id.clone();
        move |_| {
            ctx.screen
                .with(|s| s.children.iter().find(|i| i.id == id).cloned())
        }
    });
    let parent_mode = Memo::new(move |_| {
        ctx.screen
            .with(|s| s.details.as_ref().map_or(Mode::Simple, |d| d.mode))
    });
    let name = move || item.with(|i| i.as_ref().map(|i| i.name.clone()).unwrap_or_default());
    let children = move || item.with(|i| i.as_ref().map_or(0, |i| i.children));
    let is_leaf = Memo::new(move |_| item.with(|i| i.as_ref().is_some_and(|i| i.is_leaf)));

    // Renombrar en el lugar.
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let input_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(input) = input_ref.get() {
            let _ = input.focus();
            input.select();
        }
    });
    let commit_rename = {
        let id = id.clone();
        move || {
            if editing.get_untracked() {
                editing.set(false);
                let args = RenameArgs {
                    id: id.clone(),
                    name: draft.get_untracked(),
                };
                ctx.run("rename", args, |_| {});
            }
        }
    };

    let name_view = {
        let id = id.clone();
        move || {
            if editing.get() {
                let on_enter = commit_rename.clone();
                let on_blur = commit_rename.clone();
                let shift = {
                    let id = id.clone();
                    move |delta: i32| {
                        let args = ShiftArgs {
                            id: id.clone(),
                            delta,
                        };
                        ctx.run("shift", args, |_| {});
                    }
                };
                let up = shift.clone();
                view! {
                    <div class="editing">
                        <input
                            class="text"
                            node_ref=input_ref
                            prop:value=move || draft.get()
                            on:input=move |ev| draft.set(event_target_value(&ev))
                            on:keydown=move |ev: KeyboardEvent| match ev.key().as_str() {
                                "Enter" => on_enter(),
                                "Escape" => editing.set(false),
                                _ => {}
                            }
                            on:blur=move |_| on_blur()
                        />
                        // `mousedown` + preventDefault: mover sin perder el foco del nombre.
                        <button
                            class="icon"
                            title="Subir"
                            on:mousedown=move |ev| {
                                ev.prevent_default();
                                up(-1)
                            }
                        >
                            "↑"
                        </button>
                        <button
                            class="icon"
                            title="Bajar"
                            on:mousedown=move |ev| {
                                ev.prevent_default();
                                shift(1)
                            }
                        >
                            "↓"
                        </button>
                    </div>
                }
                .into_any()
            } else {
                let id = id.clone();
                let meta = move || {
                    item.with(|i| {
                        let Some(i) = i else { return Vec::new() };
                        let mut tags = Vec::new();
                        if let Some(e) = &i.effect {
                            tags.push(("effect", e.clone()));
                        }
                        for b in &i.badges {
                            tags.push(("badge", b.clone()));
                        }
                        if let Some(n) = &i.weight_note {
                            tags.push(("note", n.clone()));
                        }
                        for w in &i.warnings {
                            tags.push(("warn", format!("⚠ {w}")));
                        }
                        tags
                    })
                };
                view! {
                    <div class="main">
                        <button class="name" title="Abrir" on:click=move |_| ctx.go(Some(id.clone()))>
                            {name}
                            {move || {
                                let n = children();
                                (n > 0).then(|| view! { <span class="count">{n}</span> })
                            }}
                        </button>
                        {move || {
                            let tags = meta();
                            (!tags.is_empty())
                                .then(|| {
                                    view! {
                                        <div class="meta">
                                            {tags
                                                .into_iter()
                                                .map(|(class, text)| view! { <span class=class>{text}</span> })
                                                .collect_view()}
                                        </div>
                                    }
                                })
                        }}
                    </div>
                }
                .into_any()
            }
        }
    };

    // Peso (si el padre pondera) o créditos (si promedia por créditos).
    let weight_view = {
        let id = id.clone();
        move || match parent_mode.get() {
            Mode::Weighted => {
                let id_w = id.clone();
                let id_b = id.clone();
                let value =
                    Signal::derive(move || item.with(|i| i.as_ref().and_then(|i| i.weight)));
                let on_set = Callback::new(move |w| ctx.update(id_w.clone(), Change::Weight(w)));
                // "Respecto a": el padre (actual) o un nivel de más arriba.
                let bases = move || {
                    ctx.screen.with(|s| {
                        s.path
                            .iter()
                            .rev()
                            .map(|c| (c.id.clone(), c.name.clone()))
                            .collect::<Vec<_>>()
                    })
                };
                let current_base = move || {
                    item.with(|i| i.as_ref().and_then(|i| i.weight_base.clone()))
                        .unwrap_or_default()
                };
                let show_bases = move || ctx.screen.with(|s| s.path.len() >= 2);
                view! {
                    <div class="weight">
                        <NumberField value on_set placeholder="%" class="pct" title="Peso en %" />
                        <span class="unit">"%"</span>
                        <Show when=show_bases>
                            {
                                let id_b = id_b.clone();
                                view! {
                                    <select
                                        class="base"
                                        title="Respecto a qué nivel se expresa el peso"
                                        on:change=move |ev| {
                                            let value = event_target_value(&ev);
                                            let base = (!value.is_empty()).then_some(value);
                                            ctx.update(id_b.clone(), Change::WeightBase(base));
                                        }
                                    >
                                        {move || {
                                            let current = current_base();
                                            bases()
                                                .into_iter()
                                                .enumerate()
                                                .map(|(i, (bid, bname))| {
                                                    let value = if i == 0 { String::new() } else { bid };
                                                    let selected = value == current;
                                                    view! {
                                                        <option value=value.clone() selected=selected>
                                                            {format!("de {bname}")}
                                                        </option>
                                                    }
                                                })
                                                .collect_view()
                                        }}
                                    </select>
                                }
                            }
                        </Show>
                    </div>
                }
                .into_any()
            }
            Mode::Credits => {
                let id = id.clone();
                let value =
                    Signal::derive(move || item.with(|i| i.as_ref().and_then(|i| i.credits)));
                let on_set = Callback::new(move |c| ctx.update(id.clone(), Change::Credits(c)));
                view! {
                    <div class="weight">
                        <NumberField value on_set placeholder="cr." class="pct" title="Créditos" />
                        <span class="unit">"cr."</span>
                    </div>
                }
                .into_any()
            }
            Mode::Simple => ().into_any(),
        }
    };

    let grade_view = {
        let id = id.clone();
        move || {
            if is_leaf.get() {
                let grade = Memo::new(move |_| item.with(|i| i.as_ref().and_then(|i| i.grade)));
                let scale = Memo::new(move |_| {
                    item.with(|i| {
                        i.as_ref().map_or(
                            Scale {
                                min: 1.0,
                                max: 7.0,
                                decimals: 1,
                            },
                            |i| i.scale,
                        )
                    })
                });
                let text = RwSignal::new(String::new());
                // Mostrar la nota guardada, salvo que lo escrito ya sea esa nota.
                Effect::new(move |_| {
                    let g = grade.get();
                    if parse_grade(&text.get_untracked()) != Ok(g) {
                        text.set(
                            g.map(|g| input_text(g, scale.get_untracked()))
                                .unwrap_or_default(),
                        );
                    }
                });
                let invalid = move || match parse_grade(&text.get()) {
                    Ok(Some(g)) => !scale.get().contains(g),
                    Ok(None) => false,
                    Err(_) => true,
                };
                let title = move || {
                    let s = scale.get();
                    format!(
                        "Nota de {} a {}",
                        format_grade(s.min, s.decimals),
                        format_grade(s.max, s.decimals)
                    )
                };
                let id = id.clone();
                view! {
                    <input
                        class="grade-input"
                        class:invalid=invalid
                        inputmode="decimal"
                        placeholder="—"
                        title=title
                        prop:value=move || text.get()
                        on:input=move |ev| text.set(event_target_value(&ev))
                        on:keydown=move |ev: KeyboardEvent| {
                            if ev.key() == "Enter" {
                                ev.prevent_default();
                                if let Some(target) = ev.target() {
                                    focus_next_grade(&target);
                                }
                            }
                        }
                        on:change=move |_| match parse_grade(&text.get_untracked()) {
                            Ok(new) if new != grade.get_untracked() => {
                                let args = GradeArgs { id: id.clone(), grade: new };
                                ctx.run("set_grade", args, |_| {});
                            }
                            Ok(_) => {}
                            Err(e) => ctx.error(e),
                        }
                    />
                }
                .into_any()
            } else {
                let display = move || {
                    item.with(|i| i.as_ref().map(|i| i.display.clone()).unwrap_or_default())
                };
                let partial = move || item.with(|i| i.as_ref().is_some_and(|i| !i.complete));
                view! { <span class="grade" class:partial=partial>{display}</span> }.into_any()
            }
        }
    };

    let rename = move |_| {
        draft.set(name());
        editing.set(true);
    };
    let delete = move |_| {
        let (id, name) = (id.clone(), name());
        let args = IdArgs { id: id.clone() };
        ctx.run("delete", args, move |ctx| {
            let toast = Toast::Deleted { id, name };
            ctx.toast.set(Some(toast.clone()));
            set_timeout(
                move || {
                    if ctx.toast.get_untracked() == Some(toast) {
                        ctx.toast.set(None);
                    }
                },
                Duration::from_secs(8),
            );
        });
    };
    let dimmed = move || {
        item.with(|i| {
            i.as_ref()
                .and_then(|i| i.effect.as_deref())
                .is_some_and(|e| e != "Reemplaza a" && !e.starts_with("Reemplaza a"))
        })
    };

    // Los botones de acción quedan fuera del orden de Tab: Tab va de nota en nota.
    view! {
        <li class="row" class:dimmed=dimmed>
            {name_view}
            {weight_view}
            {grade_view}
            <div class="actions">
                <button class="icon" tabindex="-1" title="Renombrar o mover" on:click=rename>
                    "✎"
                </button>
                <button class="icon" tabindex="-1" title="Borrar" on:click=delete>
                    "🗑"
                </button>
            </div>
        </li>
    }
}

// ------------------------------------------------------------------ ajustes

/// Panel de ajustes del nodo actual: objetivo, promedio, escala y reglas opcionales.
#[component]
fn SettingsPanel() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let details = Memo::new(move |_| ctx.screen.with(|s| s.details.clone()));
    let has_children = Memo::new(move |_| ctx.screen.with(|s| !s.children.is_empty()));
    let get = move |f: fn(&Details) -> Option<f64>| {
        Signal::derive(move || details.with(|d| d.as_ref().and_then(f)))
    };

    let target = get(|d| d.target);
    let set_target = Callback::new(move |t| ctx.update_current(Change::Target(t)));
    let credits = get(|d| d.credits);
    let set_credits = Callback::new(move |c| ctx.update_current(Change::Credits(c)));
    let child_credits = move || {
        details
            .with(|d| d.as_ref().and_then(|d| d.child_credits))
            .map(|c| {
                format!(
                    "Sin créditos propios: suma {} de lo de adentro.",
                    format_number(c)
                )
            })
    };
    let has_parent = move || details.with(|d| d.as_ref().is_some_and(|d| d.has_parent));

    let mode = move || details.with(|d| d.as_ref().map_or(Mode::Simple, |d| d.mode));
    let mode_option = move |m: Mode, label: &'static str, hint: &'static str| {
        view! {
            <label class="option" title=hint>
                <input
                    type="radio"
                    name="mode"
                    prop:checked=move || mode() == m
                    on:change=move |_| ctx.update_current(Change::Mode(m))
                />
                {label}
            </label>
        }
    };

    let round = move || details.with(|d| d.as_ref().and_then(|d| d.round_result));

    view! {
        <section class="panel">
            <div class="field">
                <span class="label">"Nota objetivo"</span>
                <NumberField value=target on_set=set_target placeholder="Sin objetivo" />
                <span class="hint">"Calcula cuánto necesitas en lo que falta."</span>
            </div>

            <Show when=move || has_children.get()>
                <div class="field">
                    <span class="label">"Promedio"</span>
                    <div class="options">
                        {mode_option(Mode::Simple, "Simple", "Todas valen lo mismo")}
                        {mode_option(Mode::Weighted, "Ponderado", "Cada una con su %")}
                        {mode_option(Mode::Credits, "Por créditos", "Según los créditos de cada una")}
                    </div>
                </div>
                <div class="field">
                    <span class="label">"Redondear esta nota"</span>
                    <select on:change=move |ev| {
                        let value = match event_target_value(&ev).as_str() {
                            "yes" => Some(true),
                            "no" => Some(false),
                            _ => None,
                        };
                        ctx.update_current(Change::RoundResult(value));
                    }>
                        <option value="" selected=move || round().is_none()>
                            "Automático"
                        </option>
                        <option value="yes" selected=move || round() == Some(true)>
                            "Sí"
                        </option>
                        <option value="no" selected=move || round() == Some(false)>
                            "No"
                        </option>
                    </select>
                    <span class="hint">
                        "Automático: se redondea si tiene objetivo o si la escala dice «cada nivel»."
                    </span>
                </div>
            </Show>

            <Show when=has_parent>
                <div class="field">
                    <span class="label">"Créditos"</span>
                    <NumberField value=credits on_set=set_credits placeholder="—" />
                    <span class="hint">
                        {move || child_credits().unwrap_or_else(|| "Para promediar por créditos en el nivel de arriba.".to_owned())}
                    </span>
                </div>
            </Show>

            <ScaleEditor details />

            <details class="rules">
                <summary>"Reglas opcionales"</summary>
                <RulesEditor details has_children />
            </details>
        </section>
    }
}

#[component]
fn ScaleEditor(details: Memo<Option<Details>>) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let own = move || details.with(|d| d.as_ref().is_some_and(|d| d.config.own));
    let settings = move || {
        details.with(|d| {
            d.as_ref()
                .map_or(ScaleSettings::SCALE_1_7, |d| d.config.settings)
        })
    };
    let origin = move || {
        details.with(|d| match d.as_ref() {
            Some(d) if d.config.own => "Escala propia de este nivel.".to_owned(),
            Some(d) => match &d.config.inherited_from {
                Some(name) => format!("Heredada de {name}."),
                None => "Escala por defecto.".to_owned(),
            },
            None => String::new(),
        })
    };
    let summary = move || {
        let s = settings();
        format!(
            "{} a {}, aprueba con {}, {} decimal{}",
            format_grade(s.min, s.decimals),
            format_grade(s.max, s.decimals),
            format_grade(s.pass, s.decimals),
            s.decimals,
            if s.decimals == 1 { "" } else { "es" }
        )
    };
    let set = move |s: ScaleSettings| ctx.update_current(Change::Config(Some(s)));
    let number = move |f: fn(&ScaleSettings) -> f64, apply: fn(&mut ScaleSettings, f64)| {
        let value = Signal::derive(move || Some(f(&settings())));
        let on_set = Callback::new(move |v: Option<f64>| {
            if let Some(v) = v {
                let mut s = settings();
                apply(&mut s, v);
                set(s);
            }
        });
        view! { <NumberField value on_set /> }
    };
    let flag = move |label: &'static str,
                     f: fn(&ScaleSettings) -> bool,
                     apply: fn(&mut ScaleSettings, bool)| {
        view! {
            <label class="option">
                <input
                    type="checkbox"
                    prop:checked=move || f(&settings())
                    on:change=move |ev| {
                        let mut s = settings();
                        apply(&mut s, event_target_checked(&ev));
                        set(s);
                    }
                />
                {label}
            </label>
        }
    };

    view! {
        <div class="field">
            <span class="label">"Escala"</span>
            <span class="hint">{origin} " " {summary}</span>
            <label class="option">
                <input
                    type="checkbox"
                    prop:checked=own
                    on:change=move |ev| {
                        let change = if event_target_checked(&ev) { Some(settings()) } else { None };
                        ctx.update_current(Change::Config(change));
                    }
                />
                "Usar una escala propia aquí (y en lo de adentro)"
            </label>
        </div>
        <Show when=own>
            <div class="scale">
                <div class="presets">
                    <button class="chip" on:click=move |_| set(ScaleSettings::SCALE_1_7)>
                        "1,0 – 7,0"
                    </button>
                    <button class="chip" on:click=move |_| set(ScaleSettings::SCALE_0_100)>
                        "0 – 100"
                    </button>
                </div>
                <div class="grid">
                    <label>"Mínima" {number(|s| s.min, |s, v| s.min = v)}</label>
                    <label>"Máxima" {number(|s| s.max, |s, v| s.max = v)}</label>
                    <label>"Aprobación" {number(|s| s.pass, |s, v| s.pass = v)}</label>
                    <label>
                        "Decimales"
                        <select on:change=move |ev| {
                            if let Ok(d) = event_target_value(&ev).parse::<u8>() {
                                let mut s = settings();
                                s.decimals = d;
                                set(s);
                            }
                        }>
                            {(0u8..=3)
                                .map(|d| {
                                    view! {
                                        <option value=d.to_string() selected=move || settings().decimals == d>
                                            {d.to_string()}
                                        </option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </label>
                </div>
                {flag("Truncar en vez de aproximar", |s| s.truncate, |s, v| s.truncate = v)}
                {flag("Redondear el promedio de cada nivel", |s| s.round_every_level, |s, v| s.round_every_level = v)}
                {flag(
                    "Al pasar a otra escala, hacer calzar la nota de aprobación",
                    |s| s.by_pass,
                    |s, v| s.by_pass = v,
                )}
            </div>
        </Show>
    }
}

#[component]
fn RulesEditor(details: Memo<Option<Details>>, has_children: Memo<bool>) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let rules = move || details.with(|d| d.as_ref().map(|d| d.rules.clone()).unwrap_or_default());
    let set = move |r: RulesSettings| ctx.update_current(Change::Rules(r));
    let has_parent = move || details.with(|d| d.as_ref().is_some_and(|d| d.has_parent));
    let siblings =
        move || details.with(|d| d.as_ref().map(|d| d.siblings.clone()).unwrap_or_default());

    // Eliminar las peores (en el padre).
    let drop = Signal::derive(move || Some(f64::from(rules().drop_lowest)));
    let set_drop = Callback::new(move |v: Option<f64>| {
        let mut r = rules();
        r.drop_lowest = v.unwrap_or(0.0).max(0.0).round() as u32;
        set(r);
    });

    // Reemplazo.
    let replaces = move || rules().replaces;
    let toggle_replaces = move |on: bool| {
        let mut r = rules();
        r.replaces = on.then(ReplaceSettings::default);
        if on {
            r.exemption = None;
        }
        set(r);
    };
    let edit_replaces = move |f: &dyn Fn(&mut ReplaceSettings)| {
        let mut r = rules();
        if let Some(rep) = r.replaces.as_mut() {
            f(rep);
            set(r);
        }
    };

    // Eximición.
    let exemption = move || rules().exemption;
    let toggle_exemption = move |on: bool| {
        let mut r = rules();
        r.exemption = on.then_some(ExemptionSettings {
            threshold: 5.5,
            keep_if_better: false,
        });
        if on {
            r.replaces = None;
        }
        set(r);
    };
    let threshold = Signal::derive(move || exemption().map(|e| e.threshold));
    let set_threshold = Callback::new(move |v: Option<f64>| {
        let mut r = rules();
        if let (Some(e), Some(v)) = (r.exemption.as_mut(), v) {
            e.threshold = v;
            set(r);
        }
    });

    // Mínimo.
    let minimum = move || rules().minimum;
    let toggle_minimum = move |on: bool| {
        let mut r = rules();
        let pass = details.with(|d| d.as_ref().map_or(4.0, |d| d.config.settings.pass));
        r.minimum = on.then_some(MinimumSettings {
            grade: pass,
            cap: None,
        });
        set(r);
    };
    let min_grade = Signal::derive(move || minimum().map(|m| m.grade));
    let set_min_grade = Callback::new(move |v: Option<f64>| {
        let mut r = rules();
        if let (Some(m), Some(v)) = (r.minimum.as_mut(), v) {
            m.grade = v;
            set(r);
        }
    });
    let min_cap = Signal::derive(move || minimum().and_then(|m| m.cap));
    let set_min_cap = Callback::new(move |v: Option<f64>| {
        let mut r = rules();
        if let Some(m) = r.minimum.as_mut() {
            m.cap = v;
            set(r);
        }
    });

    view! {
        <p class="hint">"Todas vienen apagadas. Actívalas solo si tu ramo las usa."</p>
        <Show when=move || has_children.get()>
            <div class="field">
                <span class="label">"Eliminar las peores notas"</span>
                <NumberField value=drop on_set=set_drop placeholder="0" />
                <span class="hint">"Cuántas de las notas más bajas de aquí adentro no cuentan."</span>
            </div>
        </Show>
        <Show
            when=has_parent
            fallback=|| view! { <p class="hint">"Las demás reglas se ponen en una evaluación (ábrela y vuelve aquí)."</p> }
        >
            <div class="rule">
                <label class="option">
                    <input
                        type="checkbox"
                        prop:checked=move || replaces().is_some()
                        on:change=move |ev| toggle_replaces(event_target_checked(&ev))
                    />
                    "Esta nota reemplaza a otra (examen, recuperativa)"
                </label>
                <Show when=move || replaces().is_some()>
                    <div class="sub">
                        <label>
                            "Reemplaza a "
                            <select on:change=move |ev| {
                                let value = event_target_value(&ev);
                                edit_replaces(&|rep| rep.target = (!value.is_empty()).then(|| value.clone()));
                            }>
                                <option value="" selected=move || replaces().is_some_and(|r| r.target.is_none())>
                                    "la peor nota"
                                </option>
                                {move || {
                                    let current = replaces().and_then(|r| r.target);
                                    siblings()
                                        .into_iter()
                                        .map(|s| {
                                            let selected = current.as_deref() == Some(s.id.as_str());
                                            view! { <option value=s.id selected=selected>{s.name}</option> }
                                        })
                                        .collect_view()
                                }}
                            </select>
                        </label>
                        <label class="option">
                            <input
                                type="checkbox"
                                prop:checked=move || replaces().is_some_and(|r| r.only_if_better)
                                on:change=move |ev| {
                                    let on = event_target_checked(&ev);
                                    edit_replaces(&|rep| rep.only_if_better = on);
                                }
                            />
                            "Solo si mejora"
                        </label>
                        <label class="option">
                            <input
                                type="checkbox"
                                prop:checked=move || replaces().is_some_and(|r| r.counts_itself)
                                on:change=move |ev| {
                                    let on = event_target_checked(&ev);
                                    edit_replaces(&|rep| rep.counts_itself = on);
                                }
                            />
                            "Cuenta también como una nota más"
                        </label>
                    </div>
                </Show>
            </div>

            <div class="rule">
                <label class="option">
                    <input
                        type="checkbox"
                        prop:checked=move || exemption().is_some()
                        on:change=move |ev| toggle_exemption(event_target_checked(&ev))
                    />
                    "Eximible: no se rinde si el promedio de lo demás alcanza"
                </label>
                <Show when=move || exemption().is_some()>
                    <div class="sub">
                        <label>
                            "Se exime desde "
                            <NumberField value=threshold on_set=set_threshold />
                        </label>
                        <label class="option">
                            <input
                                type="checkbox"
                                prop:checked=move || exemption().is_some_and(|e| e.keep_if_better)
                                on:change=move |ev| {
                                    let mut r = rules();
                                    if let Some(e) = r.exemption.as_mut() {
                                        e.keep_if_better = event_target_checked(&ev);
                                        set(r);
                                    }
                                }
                            />
                            "Si igual se rinde y sube el promedio, contarla"
                        </label>
                    </div>
                </Show>
            </div>

            <div class="rule">
                <label class="option">
                    <input
                        type="checkbox"
                        prop:checked=move || minimum().is_some()
                        on:change=move |ev| toggle_minimum(event_target_checked(&ev))
                    />
                    "Nota mínima para aprobar el nivel de arriba"
                </label>
                <Show when=move || minimum().is_some()>
                    <div class="sub">
                        <label>
                            "Mínimo "
                            <NumberField value=min_grade on_set=set_min_grade />
                        </label>
                        <label>
                            "Si no se cumple, la nota de arriba queda en a lo más "
                            <NumberField value=min_cap on_set=set_min_cap placeholder="sin tope" />
                        </label>
                    </div>
                </Show>
            </div>
        </Show>
    }
}

// ------------------------------------------------------------------ agregar y avisos

#[component]
fn AddForm() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let name = RwSignal::new(String::new());
    let add = move || {
        let text = name.get_untracked();
        if text.trim().is_empty() {
            return;
        }
        name.set(String::new());
        let parent = ctx.current_id();
        ctx.run("create", CreateArgs { parent, name: text }, |_| {});
    };
    let placeholder = move || {
        if ctx.screen.with(|s| s.path.is_empty()) {
            "Nueva carrera, año o ramo…"
        } else {
            "Nueva evaluación o sección…"
        }
    };
    view! {
        <div class="add">
            <input
                class="text"
                placeholder=placeholder
                prop:value=move || name.get()
                on:input=move |ev| name.set(event_target_value(&ev))
                on:keydown=move |ev: KeyboardEvent| {
                    if ev.key() == "Enter" {
                        add()
                    }
                }
            />
            <button class="primary" on:click=move |_| add()>
                "Agregar"
            </button>
        </div>
    }
}

#[component]
fn ToastView() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    move || {
        ctx.toast.get().map(|toast| match toast {
            Toast::Error(message) => view! {
                <div class="toast error" role="alert">
                    <span>{message}</span>
                    <button on:click=move |_| ctx.toast.set(None)>"Cerrar"</button>
                </div>
            }
            .into_any(),
            Toast::Deleted { id, name } => view! {
                <div class="toast" role="status">
                    <span>{format!("Se borró «{name}».")}</span>
                    <button on:click=move |_| {
                        ctx.toast.set(None);
                        ctx.run("restore", IdArgs { id: id.clone() }, |_| {});
                    }>"Deshacer"</button>
                </div>
            }
            .into_any(),
        })
    }
}
