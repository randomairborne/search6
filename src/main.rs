#![warn(clippy::all, clippy::nursery, clippy::pedantic)]
mod handlers;
mod oauth;
mod reload;
mod util;
use axum::{
    response::{Html, IntoResponse},
    routing::get,
};
use deadpool_redis::{Config, Runtime};
use oauth2::{AuthUrl, ClientId, ClientSecret, RedirectUrl, RevocationUrl, TokenUrl};
use std::sync::Arc;
use twilight_model::id::{marker::WebhookMarker, Id};
use twilight_util::builder::embed::image_source::ImageSourceUrlError;
use xpd_rank_card::SvgState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let client_id = std::env::var("CLIENT_ID").expect("Expected client ID in environment");
    let client_secret =
        std::env::var("CLIENT_SECRET").expect("Expected client secret in environment");
    let root = std::env::var("ROOT").expect("Expected root in environment");
    let redis_url = std::env::var("REDIS_URL").expect("Expected redis url in environment");
    let hook_id_str = std::env::var("HOOK_ID").expect("Expected hook id in environment");
    let hook_token = std::env::var("HOOK_TOKEN").expect("Expected hook token in environment");
    let hook_id = Id::<WebhookMarker>::new(hook_id_str.parse().expect("Expected hook id: u64"));
    let http = reqwest::Client::new();
    let mut tera = tera::Tera::default();
    tera.add_raw_templates(vec![
        ("index.html", include_str!("index.html")),
    ])
    .unwrap();
    let redis_cfg = Config::from_url(redis_url);
    let redis = redis_cfg.create_pool(Some(Runtime::Tokio1)).unwrap();
    let oauth = oauth2::basic::BasicClient::new(
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
        AuthUrl::new("https://discord.com/oauth2/authorize".to_string()).unwrap(),
        Some(TokenUrl::new("https://discord.com/api/oauth2/token".to_string()).unwrap()),
    )
    .set_revocation_uri(
        RevocationUrl::new("https://discord.com/api/oauth2/token/revoke".to_string()).unwrap(),
    )
    // Set the URL the user will be redirected to after the authorization process.
    .set_redirect_uri(RedirectUrl::new(format!("{}/oc", root.trim_end_matches('/'))).unwrap());
    let hook = Arc::new(twilight_http::client::ClientBuilder::new().build());
    let state = AppState {
        tera: Arc::new(tera),
        oauth,
        svg: SvgState::new(),
        http,
        redis,
        hook,
        hook_data: Arc::new((hook_id, hook_token)),
    };
    tokio::spawn(reload::reload_loop(state.clone()));
    let app = axum::Router::new()
        .route("/", get(handlers::fetch_user))
        .route("/api", get(handlers::fetch_json))
        .route("/c", get(handlers::fetch_card))
        .route("/card", get(handlers::fetch_card))
        .route("/o", get(oauth::redirect))
        .route("/oc", get(oauth::set_id))
        .route("/style.css", get(handlers::style))
        .route("/mee6_bad.png", get(handlers::logo))
        .with_state(state);
    println!("Listening on http://localhost:8080/");
    axum::Server::bind(&([0, 0, 0, 0], 8080).into())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct User {
    pub xp: u64,
    pub id: u64,
    pub username: String,
    pub discriminator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u64>,
    pub rank: i64,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Player {
    pub xp: u64,
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub message_count: Option<u64>,
    pub avatar: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Players {
    pub players: Vec<Player>,
}

#[derive(Clone)]
pub struct AppState {
    pub tera: Arc<tera::Tera>,
    pub oauth: oauth2::basic::BasicClient,
    pub http: reqwest::Client,
    pub svg: SvgState,
    pub redis: deadpool_redis::Pool,
    pub hook: Arc<twilight_http::Client>,
    pub hook_data: Arc<(Id<WebhookMarker>, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Tera error: {0:?}")]
    Tera(#[from] tera::Error),
    #[error("Reqwest error: {0:?}")]
    Reqwest(#[from] reqwest::Error),
    #[error("SVG error: {0:?}")]
    Svg(#[from] xpd_rank_card::Error),
    #[error("Redis error: {0:?}")]
    Redis(#[from] deadpool_redis::redis::RedisError),
    #[error("Redis connection pool error: {0:?}")]
    RedisPooling(#[from] deadpool_redis::PoolError),
    #[error("JSON deserialization error: {0:?}")]
    Json(#[from] serde_json::Error),
    #[error("Twilight-HTTP error: {0:?}")]
    Twilight(#[from] twilight_http::Error),
    #[error("Twilight-Validate error: {0:?}")]
    TwilightValidate(#[from] twilight_validate::message::MessageValidationError),
    #[error("Twilight-ImageSource error: {0:?}")]
    TwilightBuilderImageSource(#[from] ImageSourceUrlError),
    #[error("ID not known- May not exist or may not be cached")]
    UnknownId,
    #[error("You must specify an ID")]
    NoId,
    #[error("This user is not ranked or may be uncached")]
    NotLevelFive,
    #[error("Invalid OAuth2 State")]
    InvalidState,
    #[error("OAuth2 Code Exchange failed")]
    CodeExchangeFailed,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let mut context = tera::Context::new();
        context.insert("error", &self.to_string());
        match tera::Tera::one_off(include_str!("error.html"), &context, true) {
            Ok(v) => Html(v).into_response(),
            Err(e) => format!(
                "There was an error while processing your request.
                Additionally, there was an error while trying to use
                an Error to nicely display the error: {e:#?}"
            )
            .into_response(),
        }
    }
}
