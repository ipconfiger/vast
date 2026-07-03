use im_server::{
    build_app, AppConfig, AppState, TlsMode,
};
use axum::{
    http::Uri,
    response::Redirect,
    routing::get,
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .init();

    tracing_log::LogTracer::init().ok();

    tracing::info!("Starting IM Server...");

    dotenvy::dotenv().ok();

    let config = AppConfig::from_env();
    let tls_mode = config.tls_mode.clone();
    std::fs::create_dir_all(&config.data_dir).ok();
    std::fs::create_dir_all(config.data_dir.join("uploads")).ok();

    let db_path = config.data_dir.join("im.db");
    let pool = im_server::db::init_pool(&db_path).await;
    let pool_for_shutdown = pool.clone();

    let ws_pool = Arc::new(im_server::ws::ConnectionPool::new());

    let state = Arc::new(AppState {
        pool,
        ws_pool,
        config,
    });

    let app = build_app(state);

    match tls_mode {
        TlsMode::None => {
            let listener =
                tokio::net::TcpListener::bind("0.0.0.0:3000")
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Failed to bind to 0.0.0.0:3000: {e}");
                    std::process::exit(1);
                });
            tracing::info!("IM Server listening on http://0.0.0.0:3000");

            let shutdown_signal = async {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("Shutdown signal received, draining connections...");
            };

            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal)
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Server error: {e}");
                    std::process::exit(1);
                });
        }
        TlsMode::SelfSigned => {
            serve_tls(app, "certs/cert.pem", "certs/key.pem").await;
        }
        TlsMode::LetsEncrypt => {
            serve_tls(app, "certs/fullchain.pem", "certs/privkey.pem").await;
        }
    }

    im_server::db::shutdown(&pool_for_shutdown).await;
}

async fn serve_tls(app: Router, cert_path: &str, key_path: &str) {
    let tls_config = RustlsConfig::from_pem_file(cert_path, key_path)
        .await
        .expect("Failed to load TLS certificate");

    let https_addr = SocketAddr::from(([0, 0, 0, 0], 3443));
    let http_addr = SocketAddr::from(([0, 0, 0, 0], 3000));

    let redirect_app = Router::new().route("/{*path}", get(http_to_https_redirect));

    tracing::info!("HTTPS server listening on https://0.0.0.0:3443");
    tracing::info!("HTTP → HTTPS redirect on http://0.0.0.0:3000");

    let https_task = tokio::spawn(
        axum_server::bind_rustls(https_addr, tls_config)
            .serve(app.into_make_service()),
    );

    let http_task = tokio::spawn(
        axum_server::bind(http_addr)
            .serve(redirect_app.into_make_service()),
    );

    tokio::signal::ctrl_c().await.ok();
    tracing::info!("Shutdown signal received, draining connections...");

    https_task.abort();
    http_task.abort();
    let _ = tokio::join!(https_task, http_task);
}

async fn http_to_https_redirect(uri: Uri) -> Redirect {
    let target = format!(
        "https://localhost:3443{}",
        uri.path_and_query().map_or("", |pq| pq.as_str())
    );
    Redirect::temporary(&target)
}
