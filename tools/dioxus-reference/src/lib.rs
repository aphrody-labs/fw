//! Owner-provided reference app (2026-11-21), built and tested by `tools/dioxus-reference/check.sh` against dioxus 0.7.10 and 0.8.0-alpha.3.
//! The changes from the owner's file are this header, the test `signal_guard_in_call_arguments_panics_and_copying_first_is_safe`, the crate-level `unreachable_code` allow for DioxusLabs/dioxus#6095, one collapsed `if` (clippy) and `strip = "debuginfo"` in the recommended profile.
//! Its catalog figures (stars, forks, issues, PRs) are a snapshot from the owner's paste; live values are in docs/research/dioxuslabs-ecosystem.json.
//!
//! # DioxusLabs 2026 Production Reference Architecture
//!
//! Dioxus 0.7 / 0.8, Signals/`generational-box`, typed Router, Fullstack `#[server]` + Axum, Manganis `asset!()`,
//! Taffy UI layout and Subsecond hot-patching. Zero `.unwrap()`/`.expect()`, zero `mod.rs`, none of the anti-patterns
//! AP-01..AP-06, no `GenerationalRef` guard held across `.await` (#6058), cancel-safe futures with an RAII guard.
//!
//! Recommended release profile:
//! ```toml
//! [profile.release]
//! opt-level = "z"
//! debug = false
//! lto = true
//! codegen-units = 1
//! panic = "abort"
//! strip = "debuginfo" # strip = true removes the wasm-bindgen custom sections: `dx build --web` then fails (checked on dx 0.7.10 with wasm-bindgen 0.2.130)
//! incremental = false
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used, unsafe_code)]
// Rust 1.100.0 makes `#[derive(Routable)]` warn "unreachable call" for String dynamic segments (DioxusLabs/dioxus#6095); the derive output is not reachable by an item-level allow.
#![allow(unreachable_code)]

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

pub static APP_STYLESHEET: Asset = asset!(
    "/assets/dioxuslabs.css",
    CssAssetOptions::new().with_minify(true)
);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum RepoCategory {
    FlagshipCore,
    RenderingAndLayout,
    ComponentsAndSdk,
    ToolingAndAutomation,
    MaintainedFork,
}

impl RepoCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::FlagshipCore => "Core & Framework",
            Self::RenderingAndLayout => "Blitz / Taffy / 2D",
            Self::ComponentsAndSdk => "Components & SDK",
            Self::ToolingAndAutomation => "CLI / Compiler / Agents",
            Self::MaintainedFork => "Upstream Fork",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RepoSummary {
    pub slug: String,
    pub category: RepoCategory,
    pub stars: u32,
    pub forks: u32,
    pub open_issues: u32,
    pub open_prs: u32,
    pub has_agents_md: bool,
    pub description: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum EcosystemError {
    InvalidFilterQuery(String),
    PathTraversalBlocked(String),
    UpstreamFailure(String),
    LayoutFailure(String),
}

impl fmt::Display for EcosystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFilterQuery(msg) => write!(f, "Requête invalide : {msg}"),
            Self::PathTraversalBlocked(raw) => write!(f, "Traversée de chemin bloquée (CWE-22) : {raw}"),
            Self::UpstreamFailure(msg) => write!(f, "Erreur serveur : {msg}"),
            Self::LayoutFailure(msg) => write!(f, "Erreur de calcul Taffy : {msg}"),
        }
    }
}

impl std::error::Error for EcosystemError {}

/// Strict validation of a static/cache path segment (CWE-22, dioxus #6046 / #6047).
pub fn validate_safe_slug(input: &str) -> Result<&str, EcosystemError> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(EcosystemError::InvalidFilterQuery("Le slug doit contenir entre 1 et 64 caractères.".to_owned()));
    }
    if trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(EcosystemError::PathTraversalBlocked(trimmed.to_owned()));
    }
    if !trimmed.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.') {
        return Err(EcosystemError::InvalidFilterQuery("Caractères ASCII alphanumériques, '-', '_' ou '.' requis.".to_owned()));
    }
    Ok(trimmed)
}

static IN_FLIGHT_REQUESTS: AtomicU64 = AtomicU64::new(0);

/// Counts requests in flight; `Drop` decrements when a component unmounts during an `.await`.
pub struct InFlightGuard {
    active: bool,
}

impl InFlightGuard {
    pub fn acquire() -> Self {
        IN_FLIGHT_REQUESTS.fetch_add(1, Ordering::Relaxed);
        Self { active: true }
    }

    pub fn current_count() -> u64 {
        IN_FLIGHT_REQUESTS.load(Ordering::Relaxed)
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if self.active {
            IN_FLIGHT_REQUESTS.fetch_sub(1, Ordering::Relaxed);
            self.active = false;
        }
    }
}

pub fn canonical_dioxuslabs_catalog() -> Vec<RepoSummary> {
    let repo = |slug: &str, category, stars, forks, open_issues, open_prs, has_agents_md, description: &str| RepoSummary {
        slug: slug.to_owned(),
        category,
        stars,
        forks,
        open_issues,
        open_prs,
        has_agents_md,
        description: description.to_owned(),
    };
    vec![
        repo("dioxus", RepoCategory::FlagshipCore, 40_000, 2_000, 674, 244, true, "Framework fullstack Rust Web, Desktop, Mobile, SSR et Native GPU."),
        repo("blitz", RepoCategory::RenderingAndLayout, 4_500, 285, 141, 329, true, "Moteur HTML/CSS modulaire pur Rust (Stylo + Taffy + Parley + anyrender)."),
        repo("taffy", RepoCategory::RenderingAndLayout, 3_801, 279, 88, 106, false, "Moteur de layout UI haute performance (Block, Flexbox, CSS Grid, Table)."),
        repo("dioxus-components", RepoCategory::ComponentsAndSdk, 356, 90, 30, 26, false, "Primitives UI accessibles non-stylées inspirées de Radix et shadcn/ui."),
        repo("sdk", RepoCategory::ComponentsAndSdk, 310, 58, 26, 9, false, "Hooks multiplateformes : presse-papiers, géolocalisation, stockage, canaux, i18n."),
        repo("rustd", RepoCategory::ToolingAndAutomation, 248, 13, 13, 8, false, "Démon de compilation incrémentale par fonction (rust-analyzer MIR + Cranelift/LLVM)."),
        repo("anyrender", RepoCategory::RenderingAndLayout, 122, 48, 13, 32, false, "Abstraction de dessin 2D Rust découplant Blitz des backends Vello et Skia."),
        repo("accessibility-cli", RepoCategory::ToolingAndAutomation, 101, 5, 0, 0, true, "CLI et crate d'automatisation computer-use multi-OS (macOS, Win UIA, AT-SPI, iOS, ADB)."),
        repo("rsrpc", RepoCategory::ToolingAndAutomation, 14, 4, 0, 1, true, "RPC Rust-vers-Rust basé sur les traits avec streaming et endpoints HTTP REST."),
    ]
}

#[server]
pub async fn fetch_org_repositories(search: String) -> Result<Vec<RepoSummary>, ServerFnError> {
    let query_norm = search.trim().to_ascii_lowercase();
    if query_norm.contains("..") {
        return Err(ServerFnError::new("Séquence '..' refusée par politique de sécurité."));
    }
    let catalog = canonical_dioxuslabs_catalog();
    if query_norm.is_empty() {
        return Ok(catalog);
    }
    Ok(catalog
        .into_iter()
        .filter(|repo| repo.slug.to_ascii_lowercase().contains(&query_norm) || repo.description.to_ascii_lowercase().contains(&query_norm))
        .collect())
}

#[server]
pub async fn inspect_repository_detail(slug: String) -> Result<RepoSummary, ServerFnError> {
    let valid_slug = validate_safe_slug(&slug).map_err(|err| ServerFnError::new(err.to_string()))?;
    canonical_dioxuslabs_catalog()
        .into_iter()
        .find(|repo| repo.slug == valid_slug)
        .ok_or_else(|| ServerFnError::new(format!("Dépôt introuvable dans DioxusLabs : {valid_slug}")))
}

#[derive(Routable, Clone, PartialEq, Eq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(AppShell)]
        #[route("/")]
        OverviewPage {},
        #[route("/repo/:slug")]
        RepoDetailPage { slug: String },
    #[end_layout]
    #[route("/:..segments")]
    NotFoundPage { segments: Vec<String> },
}

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

#[component]
pub fn App() -> Element {
    rsx! {
        document::Stylesheet { href: APP_STYLESHEET }
        document::Title { "DioxusLabs — Observatoire Architecture 2026" }
        ErrorBoundary {
            handle_error: |errors: ErrorContext| rsx! {
                section {
                    class: "error-banner",
                    role: "alert",
                    h2 { "Erreur interceptée par ErrorBoundary" }
                    p { "{errors.error():?}" }
                }
            },
            Router::<Route> {}
        }
    }
}

#[component]
pub fn AppShell() -> Element {
    // Generated once at mount, never on re-render (AP-05).
    let session_badge_id = use_hook(|| NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed));
    rsx! {
        div {
            class: "app-shell",
            "data-session-instance": "{session_badge_id}",
            header {
                class: "topbar",
                nav {
                    aria_label: "Navigation principale",
                    Link { to: Route::OverviewPage {}, "Catalogue DioxusLabs (78 dépôts)" }
                }
            }
            main {
                class: "main-content",
                SuspenseBoundary {
                    fallback: |_| rsx! { div { class: "loading-skeleton", "Chargement SSR / Hydratation..." } },
                    Outlet::<Route> {}
                }
            }
        }
    }
}

#[component]
pub fn OverviewPage() -> Element {
    // Separate fine-grained signals (AP-04).
    let mut search_query = use_signal(String::new);
    let mut only_agents_md = use_signal(|| false);
    let mut selected_category = use_signal(|| Option::<RepoCategory>::None);

    let repos_resource = use_server_future(move || {
        // Local copy before the async call: no GenerationalRef guard across .await (#6058).
        let current_query = search_query();
        async move {
            let _guard = InFlightGuard::acquire();
            fetch_org_repositories(current_query).await
        }
    })?;

    // Derived state through use_memo, never .set() in the render body (AP-03).
    let filtered_metrics = use_memo(move || {
        let require_agents = only_agents_md();
        let category_filter = selected_category();
        let guard = repos_resource.read();
        let Some(Ok(list)) = guard.as_ref() else {
            return (Vec::new(), 0_u32, 0_u32);
        };
        let mut total_stars = 0_u32;
        let mut total_prs = 0_u32;
        let mut out = Vec::with_capacity(list.len());
        for item in list {
            if require_agents && !item.has_agents_md {
                continue;
            }
            if let Some(cat) = category_filter
                && item.category != cat
            {
                continue;
            }
            total_stars = total_stars.saturating_add(item.stars);
            total_prs = total_prs.saturating_add(item.open_prs);
            out.push(item.clone());
        }
        (out, total_stars, total_prs)
    });

    let (visible_repos, stars_sum, prs_sum) = filtered_metrics();

    rsx! {
        section {
            class: "overview-grid",
            header {
                h1 { "Écosystème DioxusLabs" }
                p { "Étoiles cumulées (sélection) : {stars_sum} | PRs ouvertes : {prs_sum}" }
            }
            div {
                class: "toolbar",
                input {
                    r#type: "search",
                    placeholder: "Filtrer un dépôt (dioxus, blitz, taffy, rustd...)",
                    value: "{search_query}",
                    oninput: move |evt| search_query.set(evt.value()),
                }
                button {
                    r#type: "button",
                    onclick: move |_| {
                        // Read finished before writing the same signal (#6058).
                        let next = !only_agents_md();
                        only_agents_md.set(next);
                    },
                    if only_agents_md() { "Filtre AGENTS.md : ACTIF" } else { "Tous les dépôts" }
                }
                button {
                    r#type: "button",
                    onclick: move |_| selected_category.set(None),
                    "Réinitialiser catégorie"
                }
            }
            // Stable business key (AP-01).
            ul {
                class: "repo-list",
                for repo in visible_repos {
                    RepoCard {
                        key: "{repo.slug}",
                        repo: repo.clone(),
                        on_select_category: move |cat| selected_category.set(Some(cat)),
                    }
                }
            }
        }
    }
}

/// Pure child: Clone + PartialEq props, no Mutex or RefCell (AP-02, AP-06).
#[component]
pub fn RepoCard(repo: RepoSummary, on_select_category: EventHandler<RepoCategory>) -> Element {
    let cat = repo.category;
    let slug_for_route = repo.slug.clone();
    rsx! {
        li {
            class: "repo-card",
            article {
                header {
                    h2 {
                        Link { to: Route::RepoDetailPage { slug: slug_for_route }, "DioxusLabs/{repo.slug}" }
                    }
                    button {
                        r#type: "button",
                        class: "badge-category",
                        onclick: move |_| on_select_category.call(cat),
                        "{cat.label()}"
                    }
                }
                p { "{repo.description}" }
                footer {
                    span { "Stars: {repo.stars}" }
                    span { " | Forks: {repo.forks}" }
                    span { " | Issues: {repo.open_issues}" }
                    span { " | PRs: {repo.open_prs}" }
                    if repo.has_agents_md { strong { " | AGENTS.md" } }
                }
            }
        }
    }
}

#[component]
pub fn RepoDetailPage(slug: String) -> Element {
    let mut audit_action = use_action(move |target_slug: String| async move {
        let _guard = InFlightGuard::acquire();
        inspect_repository_detail(target_slug).await
    });
    let slug_trigger = slug.clone();
    rsx! {
        section {
            class: "repo-detail",
            h1 { "Inspection : DioxusLabs/{slug}" }
            button {
                r#type: "button",
                disabled: audit_action.pending(),
                onclick: move |_| { audit_action.call(slug_trigger.clone()); },
                if audit_action.pending() { "Audit RPC en cours..." } else { "Exécuter l'audit RPC #[server]" }
            }
            match audit_action.value() {
                Some(Ok(summary)) => {
                    let info = summary.read();
                    rsx! {
                        div {
                            class: "audit-result",
                            p { "Catégorie : {info.category.label()}" }
                            p { "Description : {info.description}" }
                            p { "Activité : {info.stars} étoiles, {info.open_prs} PRs, {info.open_issues} issues" }
                        }
                    }
                }
                Some(Err(err)) => rsx! { p { class: "audit-error", role: "alert", "Échec de l'audit : {err}" } },
                None => rsx! { p { "Cliquez sur le bouton pour interroger l'endpoint #[server]." } },
            }
            footer { Link { to: Route::OverviewPage {}, "Retour au catalogue" } }
        }
    }
}

#[component]
pub fn NotFoundPage(segments: Vec<String>) -> Element {
    let joined = segments.join("/");
    rsx! {
        section {
            class: "not-found",
            h1 { "404 — Route introuvable" }
            p { "Chemin demandé : /{joined}" }
            Link { to: Route::OverviewPage {}, "Retour à l'accueil" }
        }
    }
}

/// Launches the app (web, desktop, mobile or fullstack Axum, by Cargo features); compatible with `dx serve --hotpatch`.
pub fn launch_application() {
    dioxus::launch(App);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn rejects_cwe22_path_traversal_slugs() {
        assert!(validate_safe_slug("../etc/passwd").is_err());
        assert!(validate_safe_slug("dioxus/../../secret").is_err());
        assert!(validate_safe_slug("").is_err());
        assert_eq!(validate_safe_slug("dioxus-components"), Ok("dioxus-components"));
    }

    #[test]
    fn in_flight_guard_decrements_on_drop() {
        let baseline = InFlightGuard::current_count();
        {
            let _guard = InFlightGuard::acquire();
            assert_eq!(InFlightGuard::current_count(), baseline + 1);
        }
        assert_eq!(InFlightGuard::current_count(), baseline);
    }

    #[test]
    fn catalog_contains_core_flagships_and_no_duplicate_slugs() {
        let catalog = canonical_dioxuslabs_catalog();
        assert!(!catalog.is_empty());
        let mut seen = std::collections::HashSet::new();
        for repo in &catalog {
            assert!(seen.insert(repo.slug.as_str()), "Slug dupliqué détecté : {}", repo.slug);
            assert!(validate_safe_slug(&repo.slug).is_ok());
        }
    }

    static UNSAFE_FORM_COMPLETED: AtomicBool = AtomicBool::new(false);
    static SAFE_FORM_COMPLETED: AtomicBool = AtomicBool::new(false);

    fn store_through_copied_handle(mut count: Signal<i32>, value: i32) {
        count.set(value);
    }

    fn guard_in_call_arguments() -> Element {
        let count = use_signal(|| 0);
        use_hook(move || {
            // DioxusLabs/dioxus#6058: the peek() guard is a temporary that lives to the end of the statement.
            store_through_copied_handle(count, *count.peek() + 1);
            UNSAFE_FORM_COMPLETED.store(true, Ordering::SeqCst);
        });
        rsx! { "{count}" }
    }

    fn value_copied_before_the_call() -> Element {
        let count = use_signal(|| 0);
        use_hook(move || {
            let next = *count.peek() + 1;
            store_through_copied_handle(count, next);
            store_through_copied_handle(count, count() + 1);
            SAFE_FORM_COMPLETED.store(true, Ordering::SeqCst);
        });
        rsx! { "{count}" }
    }

    /// The runtime catches a panic raised while a component renders, so the test observes whether the hook ran to its end.
    #[test]
    fn signal_guard_in_call_arguments_panics_and_copying_first_is_safe() {
        VirtualDom::new(guard_in_call_arguments).rebuild_in_place();
        assert!(!UNSAFE_FORM_COMPLETED.load(Ordering::SeqCst), "a guard alive in the arguments must hit AlreadyBorrowed");
        VirtualDom::new(value_copied_before_the_call).rebuild_in_place();
        assert!(SAFE_FORM_COMPLETED.load(Ordering::SeqCst), "copying the value out before the call must not panic");
    }
}
