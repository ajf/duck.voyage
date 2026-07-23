//! The Axum binary: config → connections → router → serve.

mod config;
mod error;
mod handlers;
mod photo_pipeline;
mod qr;
mod state;
mod version;
mod views;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request};
use axum::http::Uri;
use axum::routing::{get, post};
use axum::{Router, ServiceExt};
use tower::Layer;
use domain::DuckCodec;
use storage::{Db, PhotoStore};
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::GovernorLayer;
use tower_http::trace::TraceLayer;
use tower_sessions::cookie::time::Duration;
use tower_sessions::{ExpiredDeletion, Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;

use crate::config::AppConfig;
use crate::photo_pipeline::PhotoPipeline;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().init();

    let config = AppConfig::from_env()?;

    let db = Db::connect(&config.database_url, config.database_max_connections).await?;
    db.migrate().await?;
    let session_store = PostgresStore::new(db.pool().clone());
    // tower-sessions' table creation has no internal lock, so serialize it
    // across concurrently booting instances.
    db.with_startup_lock(session_store.migrate()).await??;
    // One expired-session sweep per instance; concurrent sweeps are harmless
    // (plain DELETEs on expiry).
    tokio::spawn(
        session_store
            .clone()
            .continuously_delete_expired(std::time::Duration::from_secs(3600)),
    );

    let codec = DuckCodec::new(config.ff1_keys.clone(), config.current_generation)?;
    let photos = match &config.storage {
        config::StorageConfig::S3 { endpoint, bucket, access_key, secret_key } => {
            PhotoStore::s3_compatible(endpoint, bucket, access_key, secret_key)?
        }
        config::StorageConfig::Local { path } => {
            std::fs::create_dir_all(path)?;
            PhotoStore::local(path)?
        }
    };
    let oidc = auth::OidcProviders::discover(config.oidc, &config.base_url).await?;

    let secure_cookies = config.base_url.starts_with("https");
    let state = AppState::new(
        db,
        codec,
        photos,
        oidc,
        config.base_url,
        config.caps,
        config.admin_identities,
    );

    // Lax, explicitly: OIDC returns are cross-site navigations, and the
    // post-login redirect chain must carry the fresh session cookie. Lax
    // still blocks cross-site POSTs, which is our CSRF backstop.
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(secure_cookies)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(30)));

    // Rate limits (§8): strict by IP where a bot can hammer pre-auth; a
    // looser general limit on mutations as backstop. DECISION: IP-keyed for
    // v1 — per-account velocity is the documented escalation, not the start.
    // Behind a reverse proxy the peer address is the proxy, so
    // TRUST_PROXY_HEADERS switches the key to forwarded-for-style headers.
    let trust_proxy = config.trust_proxy_headers;

    let public_routes = Router::new()
        .route("/", get(handlers::public::front))
        .route("/healthz", get(handlers::public::healthz))
        .route("/missing", get(handlers::public::missing))
        .route("/static/htmx.min.js", get(handlers::public::htmx_js))
        .route("/login", get(handlers::auth_routes::login_page))
        .route("/me", get(handlers::me::me))
        .route("/me/flocks", get(handlers::me::my_flocks))
        .route("/d/{code}", get(handlers::duck::page))
        .route("/d/{code}/qr.png", get(handlers::duck::qr_png))
        .route("/d/{code}/photo", get(handlers::duck::origin_photo))
        .route(
            "/d/{code}/sightings/{id}/photo",
            get(handlers::duck::sighting_photo),
        );

    let login_routes = rate_limited(
        Router::new()
            .route("/login/{provider}", get(handlers::auth_routes::begin))
            .route(
                "/auth/callback/{provider}",
                get(handlers::auth_routes::callback_get)
                    .post(handlers::auth_routes::callback_post),
            ),
        2,
        10,
        trust_proxy,
    );

    let mutation_routes = rate_limited(
        Router::new()
            .route("/logout", post(handlers::auth_routes::logout))
            .route(
                "/me/notifications/read",
                post(handlers::me::mark_notifications_read),
            )
            .route("/flocks", post(handlers::flock::create))
            .route("/flocks/{id}/ducks", post(handlers::flock::mint))
            .route("/d/{code}/originate", post(handlers::duck::originate))
            .route("/d/{code}/set-sail", post(handlers::duck::set_sail))
            .route("/d/{code}/sightings", post(handlers::duck::log_sighting))
            .route("/d/{code}/comments", post(handlers::duck::comment))
            .route(
                "/d/{code}/follow",
                post(handlers::duck::follow).delete(handlers::duck::unfollow),
            )
            .route("/d/{code}/unfollow", post(handlers::duck::unfollow))
            .route("/d/{code}/delete", post(handlers::duck::delete_duck))
            .route("/d/{code}/restore", post(handlers::duck::restore_duck))
            .route("/d/{code}/comments/lock", post(handlers::duck::lock_comments))
            .route("/d/{code}/comments/unlock", post(handlers::duck::unlock_comments))
            .route(
                "/d/{code}/comments/{id}/delete",
                post(handlers::duck::delete_comment),
            )
            .route(
                "/d/{code}/sightings/{id}/delete",
                post(handlers::duck::delete_sighting),
            )
            .layer(DefaultBodyLimit::max(PhotoPipeline::MAX_UPLOAD_BYTES + 64 * 1024)),
        2,
        30,
        trust_proxy,
    );

    let router = Router::new()
        .merge(public_routes)
        .merge(login_routes)
        .merge(mutation_routes)
        .layer(session_layer)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // The scan-path rewrite must wrap the *router* (middleware added with
    // Router::layer runs after routing, too late to change the matched path).
    let app = axum::middleware::map_request(uppercase_scan_path).layer(router);

    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!(
        version = version::BuildInfo::VERSION,
        sha = version::BuildInfo::SHA,
        "listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(
        listener,
        ServiceExt::<Request>::into_make_service_with_connect_info::<SocketAddr>(app),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

/// Resolve on SIGTERM (how container platforms stop us) or ctrl-c, letting
/// in-flight requests finish instead of eating a kill timeout.
async fn shutdown_signal() {
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler installs");
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    tracing::info!("shutdown signal received, draining");
}

/// Apply an IP-keyed rate limit to a route group. With `trust_proxy` the key
/// comes from forwarded-for-style headers (correct behind a reverse proxy);
/// otherwise from the TCP peer address.
fn rate_limited(
    router: Router<AppState>,
    per_second: u64,
    burst: u32,
    trust_proxy: bool,
) -> Router<AppState> {
    if trust_proxy {
        let config = Arc::new(
            GovernorConfigBuilder::default()
                .key_extractor(SmartIpKeyExtractor)
                .per_second(per_second)
                .burst_size(burst)
                .finish()
                .expect("valid governor config"),
        );
        router.layer(GovernorLayer::new(config))
    } else {
        let config = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(per_second)
                .burst_size(burst)
                .finish()
                .expect("valid governor config"),
        );
        router.layer(GovernorLayer::new(config))
    }
}

/// QR labels encode `/D/CODE` (ALL CAPS keeps the QR in alphanumeric mode);
/// the router speaks lowercase. Rewrite before routing.
async fn uppercase_scan_path(mut req: Request) -> Request {
    let uri = req.uri();
    if let Some(rest) = uri.path().strip_prefix("/D/") {
        let path = format!("/d/{rest}");
        let path_and_query = match uri.query() {
            Some(q) => format!("{path}?{q}"),
            None => path,
        };
        let mut parts = uri.clone().into_parts();
        if let Ok(pq) = path_and_query.parse() {
            parts.path_and_query = Some(pq);
            if let Ok(new_uri) = Uri::from_parts(parts) {
                *req.uri_mut() = new_uri;
            }
        }
    }
    req
}
