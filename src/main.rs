#![warn(clippy::all, clippy::nursery, clippy::pedantic)]
mod handlers;
mod scores;
mod util;
use axum::{
    response::{Html, IntoResponse},
    routing::get,
};
use oauth2::{
    AuthUrl, ClientId, ClientSecret, PkceCodeVerifier, RedirectUrl, RevocationUrl, TokenUrl,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use xpd_rank_card::SvgState;

use crate::scores::Scores;
mod oauth;

#[tokio::main]
async fn main() {
    let client_id = std::env::var("CLIENT_ID").expect("Expected client ID in environment");
    let client_secret =
        std::env::var("CLIENT_SECRET").expect("Expected client secret in environment");
    let root = std::env::var("ROOT").expect("Expected root in environment");
    let http = reqwest::Client::new();
    let mut tera = tera::Tera::default();
    tera.add_raw_template("index.html", include_str!("index.html"))
        .unwrap();
    tera.add_raw_template("health.html", include_str!("health.html"))
        .unwrap();
    let client = oauth2::basic::BasicClient::new(
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
    let state = AppState {
        scores: Arc::new(RwLock::new(Scores::new(Vec::new()))),
        tera: Arc::new(tera),
        tokens: Arc::new(RwLock::new(HashMap::with_capacity(2))),
        client,
        svg: SvgState::new(),
        http,
        err: Arc::new(RwLock::new(None)),
    };
    tokio::spawn(reload_loop(state.clone()));
    let app = axum::Router::new()
        .route("/", get(handlers::fetch_user))
        .route("/c", get(handlers::fetch_card))
        .route("/card", get(handlers::fetch_card))
        .route("/health", get(handlers::health))
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

pub async fn reload_loop(state: AppState) {
    let mut timer = tokio::time::interval(std::time::Duration::from_secs(3));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut page = 0;
    let mut rank = 1;
    loop {
        timer.tick().await;
        let resp = match state
            .http
            .get("https://mee6.xyz/api/plugins/levels/leaderboard/302094807046684672")
            .query(&[("limit", 1000), ("page", page)])
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                *state.err.write().await = Some(format!("error getting users: {e:?}"));
                break;
            }
        };
        let players: Players = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                *state.err.write().await = Some(format!("error deserializing users: {e:?}"));
                break;
            }
        };
        for player in players.players {
            if player.xp < 100 {
                rank = 1;
                page = 0;
                break;
            }
            let Ok(id) = player.id.parse::<u64>() else {
                break;
            };
            let user = User {
                xp: player.xp,
                id,
                username: player.username,
                discriminator: player.discriminator,
                avatar: player.avatar,
                rank,
            };
            state.scores.write().await.insert(user);
            rank += 1;
        }
        page += 1;
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct User {
    pub xp: u64,
    pub id: u64,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
    pub rank: i64,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Player {
    pub xp: u64,
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Players {
    pub players: Vec<Player>,
}

#[derive(Clone)]
pub struct AppState {
    pub scores: Arc<RwLock<Scores>>,
    pub tera: Arc<tera::Tera>,
    pub tokens: Arc<RwLock<HashMap<String, PkceCodeVerifier>>>,
    pub client: oauth2::basic::BasicClient,
    pub http: reqwest::Client,
    pub svg: SvgState,
    pub err: Arc<RwLock<Option<String>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Tera error: {0:?}")]
    Tera(#[from] tera::Error),
    #[error("Reqwest error: {0:?}")]
    Reqwest(#[from] reqwest::Error),
    #[error("SVG error: {0:?}")]
    Svg(#[from] xpd_rank_card::Error),
    #[error("ID not known- May not exist or may not be level 5+")]
    UnknownId,
    #[error("This user is not level 5 or higher")]
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
