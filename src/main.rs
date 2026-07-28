use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, sqlx::FromRow)]
struct Animal {
    id: i32,
    name: String,
    species: String,
    breed: Option<String>,
    sex: Option<String>,
    age: Option<i32>,
    weight: Option<f64>,
    owner_name: String,
    owner_phone: Option<String>,
    medical_notes: Option<String>,
}

#[derive(Deserialize)]
struct NewAnimal {
    name: String,
    species: String,
    breed: Option<String>,
    sex: Option<String>,
    age: Option<i32>,
    weight: Option<f64>,
    owner_name: String,
    owner_phone: Option<String>,
    medical_notes: Option<String>,
}

struct RateLimiter {
    hits: Mutex<HashMap<String, (u32, Instant)>>,
    max: u32,
    window: Duration,
}

impl RateLimiter {
    fn new(max: u32, window_secs: u64) -> Self {
        Self {
            hits: Mutex::new(HashMap::new()),
            max,
            window: Duration::from_secs(window_secs),
        }
    }

    fn allow(&self, key: &str) -> bool {
        let mut map = self.hits.lock().unwrap();
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) > self.window {
            *entry = (0, now);
        }
        if entry.0 >= self.max {
            false
        } else {
            entry.0 += 1;
            true
        }
    }
}

#[derive(Clone)]
struct AppState {
    db: PgPool,
    api_key: String,
    limiter: Arc<RateLimiter>,
}

fn validate(a: &NewAnimal) -> Result<(), String> {
    if a.name.trim().is_empty() || a.name.len() > 100 {
        return Err("name must be 1-100 chars".into());
    }
    if a.species.trim().is_empty() || a.species.len() > 50 {
        return Err("species must be 1-50 chars".into());
    }
    if a.owner_name.trim().is_empty() || a.owner_name.len() > 100 {
        return Err("owner_name must be 1-100 chars".into());
    }
    if let Some(age) = a.age {
        if !(0..=200).contains(&age) {
            return Err("age must be 0-200".into());
        }
    }
    if let Some(w) = a.weight {
        if !(0.0..=2000.0).contains(&w) {
            return Err("weight must be 0-2000".into());
        }
    }
    if let Some(ref s) = a.sex {
        if s != "M" && s != "F" && s != "unknown" {
            return Err("sex must be M, F, or unknown".into());
        }
    }
    if a.breed.as_ref().map_or(false, |b| b.len() > 100) {
        return Err("breed too long".into());
    }
    if a.owner_phone.as_ref().map_or(false, |p| p.len() > 30) {
        return Err("phone too long".into());
    }
    if a.medical_notes.as_ref().map_or(false, |n| n.len() > 5000) {
        return Err("medical_notes too long".into());
    }
    Ok(())
}

async fn auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok());
    match header {
        Some(v) if v == format!("Bearer {}", state.api_key) => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn rate_limit(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if state.limiter.allow(&addr.ip().to_string()) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::TOO_MANY_REQUESTS)
    }
}

async fn serve_index() -> impl IntoResponse {
    Html(include_str!("../index.html"))
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn create_animal(
    State(state): State<AppState>,
    Json(payload): Json<NewAnimal>,
) -> impl IntoResponse {
    if let Err(e) = validate(&payload) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    let result = sqlx::query_as::<_, Animal>(
        "INSERT INTO animals
        (name, species, breed, sex, age, weight, owner_name, owner_phone, medical_notes)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *",
    )
    .bind(payload.name)
    .bind(payload.species)
    .bind(payload.breed)
    .bind(payload.sex)
    .bind(payload.age)
    .bind(payload.weight)
    .bind(payload.owner_name)
    .bind(payload.owner_phone)
    .bind(payload.medical_notes)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(a) => (StatusCode::CREATED, Json(a)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn list_animals(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_as::<_, Animal>("SELECT * FROM animals ORDER BY id")
        .fetch_all(&state.db)
        .await
    {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_animal(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    match sqlx::query_as::<_, Animal>("SELECT * FROM animals WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(a)) => (StatusCode::OK, Json(a)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn update_animal(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<NewAnimal>,
) -> impl IntoResponse {
    if let Err(e) = validate(&payload) {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    let result = sqlx::query_as::<_, Animal>(
        "UPDATE animals SET
        name=$1, species=$2, breed=$3, sex=$4, age=$5,
        weight=$6, owner_name=$7, owner_phone=$8, medical_notes=$9
        WHERE id=$10 RETURNING *",
    )
    .bind(payload.name)
    .bind(payload.species)
    .bind(payload.breed)
    .bind(payload.sex)
    .bind(payload.age)
    .bind(payload.weight)
    .bind(payload.owner_name)
    .bind(payload.owner_phone)
    .bind(payload.medical_notes)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(a)) => (StatusCode::OK, Json(a)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn delete_animal(State(state): State<AppState>, Path(id): Path<i32>) -> impl IntoResponse {
    match sqlx::query("DELETE FROM animals WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => StatusCode::NO_CONTENT.into_response(),
        Ok(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[tokio::main]
async fn main() {
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let api_key = env::var("API_KEY").expect("API_KEY must be set");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("db connect failed");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS animals (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            species TEXT NOT NULL,
            breed TEXT,
            sex TEXT,
            age INT,
            weight DOUBLE PRECISION,
            owner_name TEXT NOT NULL,
            owner_phone TEXT,
            medical_notes TEXT
        )",
    )
    .execute(&pool)
    .await
    .expect("table create failed");

    let state = AppState {
        db: pool,
        api_key,
        limiter: Arc::new(RateLimiter::new(100, 60)),
    };

    let protected = Router::new()
        .route("/animals", get(list_animals).post(create_animal))
        .route(
            "/animals/:id",
            get(get_animal).put(update_animal).delete(delete_animal),
        )
        .layer(middleware::from_fn_with_state(state.clone(), auth));

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/health", get(health))
        .merge(protected)
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    println!("listening on 0.0.0.0:3000");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
