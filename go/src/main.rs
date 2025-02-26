use std::time::Instant;

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use heed::{types::Str, Database, Env, EnvOpenOptions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower::ServiceBuilder;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Clone)]
struct AppState {
    /// The LMDB environment (wrapped in an Arc for thread safety)
    env: Arc<Env>,
    /// The LMDB database to store our requests
    db: Database<Str, Str>,
}

trait WithError {
    fn set_error(message: Option<String>) -> Self;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = tempfile::tempdir()?;

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    info!("go.silly serving on port 3000....");

    // Create (or open) the LMDB environment in the "data.mdb" directory.
    let env = Arc::new(unsafe {
        EnvOpenOptions::new()
            .max_dbs(1) // Set the maximum number of databases
            .open(path)?
    });

    let mut wtxn = env.write_txn()?;
    let db: Database<Str, Str> = env.create_database(&mut wtxn, Some("requests"))?;
    wtxn.commit()?; // Commit the transaction after creating the database.

    let app_state = AppState { env, db };

    let app = Router::new()
        .route("/{alias}", get(root))
        .route("/", post(shorten))
        .layer(ServiceBuilder::new().layer(middleware::from_fn(logging_middleware)))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

async fn logging_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();

    let method = req.method().clone();
    let uri = req.uri().clone();
    info!("Incoming request: {} {}", method, uri);

    let response: Response<Body> = next.run(req).await;

    let latency = start.elapsed();
    info!("Processed request: {} {} in {:?}", method, uri, latency);

    response
}

async fn root(State(state): State<AppState>, Path(alias): Path<String>) -> impl IntoResponse {
    let env = state.env.clone();
    let db = state.db;
    let rtxn = env.read_txn().unwrap();

    match db.get(&rtxn, &alias) {
        Ok(Some(value)) => Redirect::temporary(value),
        Ok(None) => Redirect::temporary("/"),
        Err(error) => {
            info!("Database read error: {}", error);
            Redirect::temporary("/")
        }
    }
}

async fn shorten(
    State(state): State<AppState>,
    Json(payload): Json<CreateShortUrl>,
) -> (StatusCode, Json<ShortUrl>) {
    let url = ShortUrl {
        url: payload.url,
        alias: payload.alias,
        error: None,
    };

    // Offload the blocking LMDB write to a blocking thread.
    let env = state.env.clone();
    let db = state.db;
    let key = url.alias.clone();
    let val = url.url.clone();

    tokio::task::spawn_blocking(move || {
        // Begin a write transaction.
        let mut wtxn = env.write_txn().expect("Failed to create write transaction");
        // Insert the key-value pair.
        db.put(&mut wtxn, key.as_str(), val.as_str())
            .expect("Failed to write to DB");
        // Commit the transaction.
        wtxn.commit().expect("Failed to commit transaction");
    })
    .await
    .expect("Blocking task panicked");

    (StatusCode::CREATED, Json(url))
}

#[derive(Deserialize, Clone)]
struct CreateShortUrl {
    url: String,
    alias: String,
}

#[derive(Serialize)]
struct ShortUrl {
    url: String,
    alias: String,
    error: Option<String>,
}

impl WithError for ShortUrl {
    fn set_error(message: Option<String>) -> Self {
        Self {
            url: String::new(),
            alias: String::new(),
            error: message,
        }
    }
}
