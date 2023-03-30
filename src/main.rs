#![warn(clippy::all, clippy::nursery, clippy::pedantic)]
use ahash::AHashMap;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
    routing::get,
};
use base64::Engine;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, PkceCodeVerifier, RedirectUrl, RevocationUrl, TokenUrl,
};
use rand::Rng;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use xpd_rank_card::SvgState;
mod oauth;

#[tokio::main]
async fn main() {
    let client_id = std::env::var("CLIENT_ID").expect("Expected client ID in environment");
    let client_secret =
        std::env::var("CLIENT_SECRET").expect("Expected client secret in environment");
    let root = std::env::var("ROOT").expect("Expected root in environment");
    println!("Fetching latest levels...");
    let http = reqwest::Client::new();
    let users: Vec<User> = get_users(&http).await.expect("Failed to fetch users");
    let scores = Scores {
        names: users
            .iter()
            .map(|v| (format!("{}#{}", v.username, v.discriminator), v.id))
            .collect(),
        ids: users.into_iter().map(|v| (v.id, v)).collect(),
    };
    println!("Fetched latest levels, starting server...");
    let mut tera = tera::Tera::default();
    tera.add_raw_template("index.html", include_str!("index.html"))
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
        scores: Arc::new(RwLock::new(scores)),
        tera: Arc::new(tera),
        tokens: Arc::new(RwLock::new(HashMap::with_capacity(2))),
        client,
        svg: SvgState::new(),
        http,
    };
    tokio::spawn(reload_loop(state.clone()));
    let app = axum::Router::new()
        .route("/", get(fetch_user))
        .route("/c", get(fetch_card))
        .route("/card", get(fetch_card))
        .route("/o", get(oauth::redirect))
        .route("/oc", get(oauth::set_id))
        .route("/mee6_bad.png", get(logo_handler))
        .with_state(state);
    println!("Listening on http://localhost:8080/");
    axum::Server::bind(&([0, 0, 0, 0], 8080).into())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[allow(clippy::unused_async)]
pub async fn logo_handler() -> ([(&'static str, &'static str); 1], &'static [u8]) {
    (
        [("Content-Type", "image/png")],
        include_bytes!("mee6_bad.png"),
    )
}

pub async fn reload_loop(state: AppState) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1200)).await;
        let users = match get_users(&state.http).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed to update users: {e}");
                break;
            }
        };
        for user in users {
            state.scores.write().await.insert(user);
        }
    }
}

#[allow(clippy::missing_errors_doc)]
pub async fn get_users(http: &reqwest::Client) -> Result<Vec<User>, Error> {
    let mut users: Vec<User> = http
        .get("https://cdn.valk.sh/mc-discord-archive/latest.json")
        .send()
        .await?
        .json()
        .await?;
    users.sort_by(|a, b| a.xp.cmp(&b.xp).reverse());
    for (rank, user) in users.iter_mut().enumerate() {
        user.rank = Some(rank + 1);
    }
    Ok(users)
}

#[allow(clippy::missing_errors_doc)]
pub async fn fetch_user(
    State(state): State<AppState>,
    Query(query): Query<SubmitQuery>,
) -> Result<Html<String>, Error> {
    let Some(id) = query.id else {
        return Ok(Html(state.tera.render("index.html", &tera::Context::new())?))
    };
    let scores = state.scores.read().await;
    let result_user = scores.get(id.trim());
    let user = if query.userexists {
        result_user.ok_or(Error::NotLevelFive)?
    } else {
        result_user.ok_or(Error::UnknownId)?
    };
    let level_info = mee6::LevelInfo::new(user.xp);
    let mut ctx = tera::Context::new();
    ctx.insert("level", &level_info.level());
    ctx.insert("percentage", &level_info.percentage());
    ctx.insert("user", &user);
    if let Some(avatar) = &user.avatar {
        let ext = if avatar.starts_with("a_") {
            "gif"
        } else {
            "png"
        };
        ctx.insert(
            "avatar",
            &format!(
                "https://cdn.discordapp.com/avatars/{}/{}.{}",
                user.id, avatar, ext
            ),
        );
    }
    Ok(Html(state.tera.render("index.html", &ctx)?))
}

#[allow(clippy::missing_errors_doc)]
pub async fn fetch_card(
    State(state): State<AppState>,
    Query(query): Query<SubmitQuery>,
) -> Result<([(&'static str, &'static str); 1], Vec<u8>), Error> {
    let Some(id) = query.id else {
        return Err(Error::UnknownId);
    };
    let scores = state.scores.read().await;
    let result_user = scores.get(id.trim());
    let user = if query.userexists {
        result_user.ok_or(Error::NotLevelFive)?
    } else {
        result_user.ok_or(Error::UnknownId)?
    };
    let level_info = mee6::LevelInfo::new(user.xp);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ctx = xpd_rank_card::Context {
        level: level_info.level(),
        rank: user.rank.unwrap_or(0) as i64,
        name: user.username.clone(),
        discriminator: user.discriminator.clone(),
        percentage: (level_info.percentage() * 100.0).round() as u64,
        current: level_info.xp(),
        needed: mee6::xp_needed_for_level(level_info.level() + 1),
        toy: None,
        avatar: get_avatar(&state, user).await?,
        font: "Mojang".to_string(),
        colors: xpd_rank_card::colors::Colors::default(),
    };
    Ok((
        [("Content-Type", "image/png")],
        state.svg.render(ctx).await?,
    ))
}

#[derive(serde::Deserialize)]
pub struct SubmitQuery {
    id: Option<String>,
    #[serde(default = "rfalse")]
    userexists: bool,
}

const fn rfalse() -> bool {
    false
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct User {
    pub xp: u64,
    pub id: u64,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
    pub rank: Option<usize>,
}

#[derive(Clone)]
pub struct AppState {
    pub scores: Arc<RwLock<Scores>>,
    pub tera: Arc<tera::Tera>,
    pub tokens: Arc<RwLock<HashMap<String, PkceCodeVerifier>>>,
    pub client: oauth2::basic::BasicClient,
    pub http: reqwest::Client,
    pub svg: SvgState,
}

pub struct Scores {
    ids: AHashMap<u64, User>,
    names: HashMap<String, u64>,
}

impl Scores {
    pub fn insert(&mut self, user: User) {
        self.names
            .insert(format!("{}#{}", user.username, user.discriminator), user.id);
        self.ids.insert(user.id, user);
    }
    #[must_use]
    pub fn get(&self, identifier: &str) -> Option<&User> {
        if let Ok(id) = identifier.parse::<u64>() {
            return self.ids.get(&id);
        }
        if let Some(id) = self.names.get(identifier) {
            return self.ids.get(id);
        }
        None
    }
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

async fn get_avatar(state: &AppState, user: &User) -> Result<String, Error> {
    let url = user.avatar.as_ref().map_or_else(
        || {
            format!(
                "https://cdn.discordapp.com/embed/avatars/{}/{}.png",
                user.id,
                user.discriminator.parse().unwrap_or(1) % 5
            )
        },
        |hash| {
            format!(
                "https://cdn.discordapp.com/avatars/{}/{}.png",
                user.id, hash
            )
        },
    );
    let png = state.http.get(url).send().await?.bytes().await?;
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    );
    Ok(data)
}
