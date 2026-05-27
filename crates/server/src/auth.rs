use std::time::{SystemTime, UNIX_EPOCH};

use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::Json;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::AppState;

const COOKIE: &str = "peekr_token";
const TTL_SECS: u64 = 60 * 60 * 24 * 7; // 7 days

// --- password hashing ---

pub fn hash_password(password: &str) -> anyhow::Result<String> {
  let salt = SaltString::generate(&mut OsRng);
  Argon2::default()
    .hash_password(password.as_bytes(), &salt)
    .map(|h| h.to_string())
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
  match PasswordHash::new(hash) {
    Ok(parsed) => Argon2::default()
      .verify_password(password.as_bytes(), &parsed)
      .is_ok(),
    Err(_) => false,
  }
}

// --- JWT ---

#[derive(Serialize, Deserialize)]
pub struct Claims {
  pub sub: i64,
  pub email: String,
  pub role: String,
  pub exp: usize,
}

pub fn make_token(secret: &[u8], id: i64, email: &str, role: &str) -> anyhow::Result<String> {
  let exp = (now() + TTL_SECS) as usize;
  let claims = Claims {
    sub: id,
    email: email.to_string(),
    role: role.to_string(),
    exp,
  };
  Ok(encode(
    &Header::default(),
    &claims,
    &EncodingKey::from_secret(secret),
  )?)
}

pub fn verify_token(secret: &[u8], token: &str) -> Option<Claims> {
  decode::<Claims>(
    token,
    &DecodingKey::from_secret(secret),
    &Validation::default(),
  )
  .ok()
  .map(|d| d.claims)
}

fn now() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0)
}

// --- DTOs ---

#[derive(FromRow)]
struct UserRow {
  id: i64,
  email: String,
  password_hash: String,
  role: String,
}

#[derive(Deserialize)]
pub struct Credentials {
  email: String,
  password: String,
}

#[derive(Serialize)]
pub struct PublicUser {
  id: i64,
  email: String,
  role: String,
}

// --- auth extractor + middleware ---

pub struct AuthUser {
  pub id: i64,
  pub email: String,
  pub role: String,
}

impl FromRequestParts<AppState> for AuthUser {
  type Rejection = StatusCode;

  async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, StatusCode> {
    let jar = CookieJar::from_headers(&parts.headers);
    let token = jar
      .get(COOKIE)
      .map(|c| c.value().to_string())
      .ok_or(StatusCode::UNAUTHORIZED)?;
    let claims = verify_token(&state.secret, &token).ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(AuthUser {
      id: claims.sub,
      email: claims.email,
      role: claims.role,
    })
  }
}

pub async fn require_auth(
  State(state): State<AppState>,
  jar: CookieJar,
  req: Request,
  next: Next,
) -> Response {
  let authed = jar
    .get(COOKIE)
    .and_then(|c| verify_token(&state.secret, c.value()))
    .is_some();
  if authed {
    next.run(req).await
  } else {
    StatusCode::UNAUTHORIZED.into_response()
  }
}

fn session_cookie(token: String) -> Cookie<'static> {
  Cookie::build((COOKIE, token))
    .http_only(true)
    .same_site(SameSite::Lax)
    .path("/")
    .max_age(time::Duration::seconds(TTL_SECS as i64))
    .build()
}

// --- handlers ---

pub async fn first_run(State(state): State<AppState>) -> Json<serde_json::Value> {
  let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
  Json(serde_json::json!({ "firstRun": count == 0 }))
}

pub async fn signup(
  State(state): State<AppState>,
  jar: CookieJar,
  Json(creds): Json<Credentials>,
) -> Result<(CookieJar, Json<PublicUser>), (StatusCode, String)> {
  let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
    .fetch_one(&state.db)
    .await
    .map_err(internal)?;
  // signup is only for the very first user; afterwards it's login-only
  if count > 0 {
    return Err((StatusCode::FORBIDDEN, "registration closed".into()));
  }
  if creds.email.trim().is_empty() || creds.password.len() < 8 {
    return Err((
      StatusCode::BAD_REQUEST,
      "email required, password >= 8 chars".into(),
    ));
  }

  let hash = hash_password(&creds.password).map_err(internal)?;
  let res = sqlx::query(
    "INSERT INTO users (email, password_hash, role, created_at) VALUES (?, ?, 'admin', ?)",
  )
  .bind(&creds.email)
  .bind(&hash)
  .bind(now() as i64)
  .execute(&state.db)
  .await
  .map_err(internal)?;
  let id = res.last_insert_rowid();

  let token = make_token(&state.secret, id, &creds.email, "admin").map_err(internal)?;
  let user = PublicUser {
    id,
    email: creds.email,
    role: "admin".into(),
  };
  Ok((jar.add(session_cookie(token)), Json(user)))
}

pub async fn login(
  State(state): State<AppState>,
  jar: CookieJar,
  Json(creds): Json<Credentials>,
) -> Result<(CookieJar, Json<PublicUser>), (StatusCode, String)> {
  let row: Option<UserRow> =
    sqlx::query_as("SELECT id, email, password_hash, role FROM users WHERE email = ?")
      .bind(&creds.email)
      .fetch_optional(&state.db)
      .await
      .map_err(internal)?;

  let Some(user) = row else {
    return Err((StatusCode::UNAUTHORIZED, "invalid credentials".into()));
  };
  if !verify_password(&creds.password, &user.password_hash) {
    return Err((StatusCode::UNAUTHORIZED, "invalid credentials".into()));
  }

  let token = make_token(&state.secret, user.id, &user.email, &user.role).map_err(internal)?;
  let public = PublicUser {
    id: user.id,
    email: user.email,
    role: user.role,
  };
  Ok((jar.add(session_cookie(token)), Json(public)))
}

pub async fn logout(jar: CookieJar) -> CookieJar {
  jar.remove(Cookie::from(COOKIE))
}

pub async fn me(user: AuthUser) -> Json<PublicUser> {
  Json(PublicUser {
    id: user.id,
    email: user.email,
    role: user.role,
  })
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
  (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn password_roundtrip() {
    let hash = hash_password("hunter2-secret").unwrap();
    assert!(verify_password("hunter2-secret", &hash));
    assert!(!verify_password("wrong", &hash));
  }

  #[test]
  fn token_roundtrip() {
    let secret = b"test-secret-key";
    let token = make_token(secret, 42, "a@b.c", "admin").unwrap();
    let claims = verify_token(secret, &token).expect("valid");
    assert_eq!(claims.sub, 42);
    assert_eq!(claims.email, "a@b.c");
    assert_eq!(claims.role, "admin");
  }

  #[test]
  fn token_rejects_wrong_secret_and_garbage() {
    let token = make_token(b"secret-a", 1, "x@y.z", "admin").unwrap();
    assert!(verify_token(b"secret-b", &token).is_none());
    assert!(verify_token(b"secret-a", "not-a-token").is_none());
  }

  #[tokio::test]
  async fn user_row_persists_and_verifies() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
      .max_connections(1)
      .connect("sqlite::memory:")
      .await
      .unwrap();
    crate::db::init_schema(&pool).await.unwrap();

    let hash = hash_password("password123").unwrap();
    sqlx::query(
      "INSERT INTO users (email, password_hash, role, created_at) VALUES (?, ?, 'admin', 0)",
    )
    .bind("a@b.c")
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
      .fetch_one(&pool)
      .await
      .unwrap();
    assert_eq!(count, 1);

    let row: UserRow =
      sqlx::query_as("SELECT id, email, password_hash, role FROM users WHERE email = ?")
        .bind("a@b.c")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(verify_password("password123", &row.password_hash));
    assert!(!verify_password("nope", &row.password_hash));
  }
}
