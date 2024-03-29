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
use serde::Deserialize;
use std::sync::Arc;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};
use twilight_model::id::{marker::GuildMarker, Id};
use twilight_util::builder::embed::image_source::{
    ImageSourceAttachmentError, ImageSourceUrlError,
};
use xpd_rank_card::SvgState;

#[macro_use]
extern crate tracing;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let log = std::env::var("LOG").unwrap_or_else(|_e| "warn,search6=info".to_string());
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new(&log))
        .init();
    let root_url = std::env::var("ROOT_URL")
        .expect("Expected a ROOT_URL in the environment")
        .trim_end_matches('/')
        .to_string();
    let guild_id: Id<GuildMarker> = std::env::var("GUILD_ID")
        .expect("Expected a GUILD_ID in the environment")
        .parse()
        .expect("Expected valid server ID in GUILD_ID");
    let redis_url = std::env::var("REDIS_URL").expect("Expected REDIS_URL in environment");
    let oauth = util::get_oauth(&root_url);
    let webhook = util::get_webhook();
    if webhook.is_none() {
        warn!("webhook functionality disabled! (if you aren't valk, you can ignore this)");
    } else {
        info!("Webhook level-up notifications enabled!");
    }
    if oauth.is_none() {
        warn!("OAuth2 functionality disabled! (if you aren't valk, you can ignore this)");
    } else {
        info!("OAuth2 enabled!");
    }
    let http = reqwest::Client::new();
    let mut tera = tera::Tera::default();
    tera.add_raw_templates(vec![("index.html", include_str!("resources/index.html"))])
        .unwrap();
    let pool_cfg = deadpool_redis::PoolConfig::new(25);
    let mut redis_cfg = Config::from_url(redis_url);
    redis_cfg.pool = Some(pool_cfg);
    let redis = redis_cfg.create_pool(Some(Runtime::Tokio1)).unwrap();
    let state = AppState {
        tera: Arc::new(tera),
        oauth,
        svg: SvgState::new(),
        http,
        redis,
        webhook,
        guild_id,
        root_url: Arc::new(root_url),
    };
    tokio::spawn(reload::reload_loop(state.clone()));
    let app = axum::Router::new()
        .route("/", get(handlers::fetch_user))
        .route("/api", get(handlers::fetch_json))
        .route("/c", get(handlers::fetch_card))
        .route("/card", get(handlers::fetch_card))
        .route("/card.svg", get(handlers::fetch_svg))
        .route("/o", get(oauth::redirect))
        .route("/oc", get(oauth::set_id))
        .route("/style.css", get(handlers::style))
        .route("/mee6_bad.png", get(handlers::mee6bad))
        .route("/search6.png", get(handlers::logo))
        .route("/minecraft.woff", get(handlers::font))
        .with_state(state);
    info!("Listening on http://localhost:8080/");
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
    #[serde(deserialize_with = "user_discrim_deserialize")]
    pub discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u64>,
    pub rank: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<i64>,
}

impl User {
    #[must_use]
    pub fn human_identifier(&self) -> String {
        self.discriminator.as_ref().map_or_else(
            || self.username.clone(),
            |discriminator| format!("{}#{}", self.username, discriminator),
        )
    }
}

fn user_discrim_deserialize<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let de = Option::<String>::deserialize(deserializer)?;
    if let Some(ref d) = de {
        if d.chars().all(|v| v == '0') {
            Ok(None)
        } else {
            Ok(de)
        }
    } else {
        Ok(de)
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Player {
    pub xp: u64,
    pub id: String,
    pub username: String,
    #[serde(deserialize_with = "user_discrim_deserialize")]
    pub discriminator: Option<String>,
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
    pub oauth: Option<oauth2::basic::BasicClient>,
    pub http: reqwest::Client,
    pub svg: SvgState,
    pub redis: deadpool_redis::Pool,
    pub webhook: Option<util::WebhookState>,
    pub guild_id: Id<GuildMarker>,
    pub root_url: Arc<String>,
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
    #[error("Twilight-ImageSource:URL error: {0:?}")]
    TwilightBuilderImageSourceUrl(#[from] ImageSourceUrlError),
    #[error("Twilight-ImageSource:Attachment error: {0:?}")]
    TwilightBuilderImageSourceAttachment(#[from] ImageSourceAttachmentError),
    #[error("ParseInt error: {0:?}")]
    ParseInt(#[from] std::num::ParseIntError),
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
    #[error("OAuth2 is disabled on this search6 instance")]
    OauthDisabled,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let mut context = tera::Context::new();
        context.insert("error", &self.to_string());
        match tera::Tera::one_off(include_str!("resources/error.html"), &context, true) {
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
