//! The Axum binary: config → connections → router → serve.

mod config;
mod error;
mod handlers;
mod photo_pipeline;
mod qr;
mod state;
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
use tower_governor::GovernorLayer;
use tower_http::trace::TraceLayer;
use tower_sessions::cookie::time::Duration;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;

use crate::config::AppConfig;
use crate::photo_pipeline::PhotoPipeline;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().init();

    let config = AppConfig::from_env()?;

    let db = Db::connect(&config.database_url).await?;
    db.migrate().await?;
    let session_store = PostgresStore::new(db.pool().clone());
    session_store.migrate().await?;

    let codec = DuckCodec::new(config.ff1_keys.clone(), config.current_generation)?;
    let photos = PhotoStore::s3_compatible(
        &config.storage.endpoint,
        &config.storage.bucket,
        &config.storage.access_key,
        &config.storage.secret_key,
    )?;
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

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(secure_cookies)
        .with_expiry(Expiry::OnInactivity(Duration::days(30)));

    // Rate limits (§8): strict by IP where a bot can hammer pre-auth; a
    // looser general limit on mutations as backstop. DECISION: IP-keyed for
    // v1 — per-account velocity is the documented escalation, not the start.
    let login_governor = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(10)
            .finish()
            .expect("valid governor config"),
    );
    let mutation_governor = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(30)
            .finish()
            .expect("valid governor config"),
    );

    let public_routes = Router::new()
        .route("/", get(handlers::public::front))
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

    let login_routes = Router::new()
        .route("/login/{provider}", get(handlers::auth_routes::begin))
        .route(
            "/auth/callback/{provider}",
            get(handlers::auth_routes::callback_get).post(handlers::auth_routes::callback_post),
        )
        .layer(GovernorLayer::new(login_governor));

    let mutation_routes = Router::new()
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
        .layer(DefaultBodyLimit::max(PhotoPipeline::MAX_UPLOAD_BYTES + 64 * 1024))
        .layer(GovernorLayer::new(mutation_governor));

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

    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], config.listen_port))).await?;
    tracing::info!("listening on http://{}", listener.local_addr()?);
    axum::serve(
        listener,
        ServiceExt::<Request>::into_make_service_with_connect_info::<SocketAddr>(app),
    )
    .await?;
    Ok(())
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
