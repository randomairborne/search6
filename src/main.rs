use ahash::AHashMap;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
    routing::get,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    println!("Fetching latest levels...");
    let levels: Vec<IncomingUser> =
        reqwest::get("https://cdn.valk.sh/mc-discord-archive/latest.json")
            .await
            .expect("Failed to fetch Minecraft Discord archive!")
            .json()
            .await
            .expect("Failed to deserialize Minecraft Discord archive!");
    let id_scores = Arc::new(RwLock::new(
        levels
            .iter()
            .map(|v| (v.id, v.xp))
            .collect::<AHashMap<u64, u64>>(),
    ));
    let name_scores = Arc::new(RwLock::new(
        levels
            .iter()
            .map(|v| (format!("{}#{}", v.username, v.discriminator), v.xp))
            .collect::<HashMap<String, u64>>(),
    ));
    let scores = Scores {
        ids: id_scores,
        names: name_scores,
    };
    println!("Fetched latest levels, starting server...");
    let mut tera = tera::Tera::default();
    tera.add_raw_template("index.html", include_str!("index.html"))
        .unwrap();
    let state = AppState {
        scores,
        tera: Arc::new(tera),
    };
    tokio::spawn(reload_loop(state.clone()));
    let app = axum::Router::new()
        .route("/", get(fetch_user))
        .route("/mee6_bad.png", get(logo_handler))
        .with_state(state);
    println!("Listening on http://localhost:8080/");
    axum::Server::bind(&([0, 0, 0, 0], 8080).into())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

pub async fn logo_handler() -> ([(&'static str, &'static str); 1], &'static [u8]) {
    (
        [("Content-Type", "image/png")],
        include_bytes!("mee6_bad.png"),
    )
}

pub async fn reload_loop(state: AppState) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1200)).await;
        let users: Vec<IncomingUser> =
            reqwest::get("https://cdn.valk.sh/mc-discord-archive/latest.json")
                .await
                .expect("Failed to fetch Minecraft Discord archive!")
                .json()
                .await
                .expect("Failed to deserialize Minecraft Discord archive!");
        for user in users {
            state.scores.insert(user).await;
        }
    }
}

pub async fn fetch_user(
    State(state): State<AppState>,
    Query(query): Query<SubmitQuery>,
) -> Result<Html<String>, Error> {
    let Some(id) = query.id else {
        return Ok(Html(state.tera.render("index.html", &tera::Context::new())?))
    };
    let xp = state.scores.get(&id).await.ok_or(Error::UnknownId)?;
    let level_info = mee6::LevelInfo::new(xp);
    let mut ctx = tera::Context::new();
    ctx.insert("level", &level_info.level());
    ctx.insert("percentage", &level_info.percentage());
    ctx.insert("xp", &level_info.xp());
    ctx.insert("id", &id);
    Ok(Html(state.tera.render("index.html", &ctx)?))
}

#[derive(serde::Deserialize)]
pub struct IncomingUser {
    pub xp: u64,
    pub id: u64,
    pub username: String,
    pub discriminator: String,
}

#[derive(Clone)]
pub struct AppState {
    pub scores: Scores,
    pub tera: Arc<tera::Tera>,
}

#[derive(Clone)]
pub struct Scores {
    ids: Arc<RwLock<AHashMap<u64, u64>>>,
    names: Arc<RwLock<HashMap<String, u64>>>,
}

impl Scores {
    pub async fn insert(&self, user: IncomingUser) {
        self.ids.write().await.insert(user.id, user.xp);
        self.names
            .write()
            .await
            .insert(format!("{}#{}", user.username, user.discriminator), user.xp);
    }
    pub async fn get(&self, identifier: &str) -> Option<u64> {
        if let Ok(id) = identifier.parse::<u64>() {
            return self.ids.read().await.get(&id).copied();
        }
        self.names.read().await.get(identifier).copied()
    }
}

#[derive(serde::Deserialize)]
pub struct SubmitQuery {
    id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Tera error: {0}")]
    Tera(#[from] tera::Error),
    #[error("ID not known- May not exist or may not be level 5+")]
    UnknownId,
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
