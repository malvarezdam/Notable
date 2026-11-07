//! Interfaz de Notable. No calcula nada: le pide a la app (comandos Tauri) la pantalla ya lista
//! y le avisa de cada cambio; después de cada cambio vuelve a pedir la pantalla.

use std::time::Duration;

use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use notable_api::{Scale, Screen, format_grade, parse_grade};
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
    };
    provide_context(ctx);
    ctx.reload();

    view! {
        <main>
            <Crumbs />
            <Summary />
            <Children />
            <AddForm />
        </main>
        <ToastView />
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

/// Tarjeta con el nombre y la nota del nodo actual.
#[component]
fn Summary() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
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
            view! {
                <section class="summary">
                    <div>
                        <h1>{item.name}</h1>
                        <div class="state">{state}</div>
                    </div>
                    <div class=class>{item.display}</div>
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
                view! {
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
                }
                .into_any()
            } else {
                let id = id.clone();
                view! {
                    <button class="name" title="Abrir" on:click=move |_| ctx.go(Some(id.clone()))>
                        {name}
                        {move || {
                            let n = children();
                            (n > 0).then(|| view! { <span class="count">{n}</span> })
                        }}
                    </button>
                }
                .into_any()
            }
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

    // Los botones de acción quedan fuera del orden de Tab: Tab va de nota en nota.
    view! {
        <li class="row">
            {name_view}
            {grade_view}
            <button class="icon" tabindex="-1" title="Renombrar" on:click=rename>"✎"</button>
            <button class="icon" tabindex="-1" title="Borrar" on:click=delete>"🗑"</button>
        </li>
    }
}

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
        let parent = ctx
            .screen
            .with_untracked(|s| s.current.as_ref().map(|c| c.id.clone()));
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
