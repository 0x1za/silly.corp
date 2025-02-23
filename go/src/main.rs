use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    // initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    info!("go.silly serving cunt....");

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `POST /users` goes to `create_user`
        .route("/create", post(shorten));

    // Run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn shorten(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateShortUrl>,
) -> (StatusCode, Json<ShortUrl>) {
    let url = ShortUrl {
        id: 1337,
        url: payload.url,
        alias: payload.alias,
    };

    // this will be converted into a JSON response
    // with a status code of `201 Created`
    (StatusCode::CREATED, Json(url))
}

// the input to our `create_user` handler
#[derive(Deserialize)]
struct CreateShortUrl {
    url: String,
    alias: String,
}

// the output to our `create_user` handler
#[derive(Serialize)]
struct ShortUrl {
    id: u64,
    url: String,
    alias: String,
}
