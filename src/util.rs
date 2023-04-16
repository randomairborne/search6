use std::sync::Arc;

use base64::Engine;
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, RevocationUrl, TokenUrl,
};
use redis::AsyncCommands;
use twilight_model::id::{marker::WebhookMarker, Id};

use crate::{AppState, Error, User};

pub async fn get_avatar_data(state: &AppState, user: &User) -> Result<String, Error> {
    let url = get_avatar_url(user.id, &user.discriminator, &user.avatar, false);
    let png = state.http.get(url).send().await?.bytes().await?;
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    );
    Ok(data)
}

pub async fn get_user(
    mut redis: deadpool_redis::Connection,
    id: String,
    user_exists: bool,
) -> Result<User, Error> {
    let user_id = if id.chars().all(|c| c.is_ascii_digit()) {
        id
    } else {
        let slug_key = format!("user.slug:{id}");
        let id: Option<String> = redis.get(slug_key).await?;
        id.ok_or(Error::UnknownId)?
    };
    let data_string_optional: Option<String> = redis.get(format!("user.id:{user_id}")).await?;
    let data_string = if user_exists {
        data_string_optional.ok_or(Error::NotLevelFive)?
    } else {
        data_string_optional.ok_or(Error::UnknownId)?
    };
    Ok(serde_json::from_str(&data_string)?)
}

pub fn get_avatar_url(id: u64, discrim: &str, hash: &Option<String>, allowgif: bool) -> String {
    let Some(hash) = hash else {
        return format!(
            "https://cdn.discordapp.com/embed/avatars/{}.png?width=256&height=256",
            // display the 5.png easter egg if we can't parse the discrim
            discrim.parse::<u16>().map_or(5, |v| v % 5)
        );
    };
    if hash.is_empty() {
        return format!(
            "https://cdn.discordapp.com/embed/avatars/{}.png?width=256&height=256",
            // display the 5.png easter egg if we can't parse the discrim
            discrim.parse::<u16>().map_or(5, |v| v % 5)
        );
    }
    let ext = if allowgif {
        if hash.starts_with("a_") {
            "gif"
        } else {
            "png"
        }
    } else {
        "png"
    };
    format!("https://cdn.discordapp.com/avatars/{id}/{hash}.{ext}")
}

pub fn get_oauth(root_url: &str) -> Option<BasicClient> {
    let client_id = std::env::var("CLIENT_ID").ok();
    let client_secret = std::env::var("CLIENT_SECRET").ok();
    if client_id.is_some() && client_secret.is_none() {
        panic!("if CLIENT_ID is set, CLIENT_SECRET and ROOT_URL must also both be set!");
    } else if client_secret.is_some() && client_id.is_none() {
        panic!("if CLIENT_SECRET is set, CLIENT_ID must also be set!")
    }
    let oauth = oauth2::basic::BasicClient::new(
        ClientId::new(client_id?),
        Some(ClientSecret::new(client_secret?)),
        AuthUrl::new("https://discord.com/oauth2/authorize".to_string()).unwrap(),
        Some(TokenUrl::new("https://discord.com/api/oauth2/token".to_string()).unwrap()),
    )
    .set_revocation_uri(
        RevocationUrl::new("https://discord.com/api/oauth2/token/revoke".to_string()).unwrap(),
    )
    // Set the URL the user will be redirected to after the authorization process.
    .set_redirect_uri(RedirectUrl::new(format!("{root_url}/oc")).unwrap());
    Some(oauth)
}

#[derive(Clone)]
pub struct WebhookState {
    pub client: Arc<twilight_http::Client>,
    pub marker: Id<WebhookMarker>,
    pub token: Arc<String>,
}

pub fn get_webhook() -> Option<WebhookState> {
    let url = std::env::var("WEBHOOK").ok()?;
    let (marker, webhook_token) =
        twilight_util::link::webhook::parse(&url).expect("Error parsing webhook URL");
    let token = Arc::new(webhook_token.expect("Missing webhook token").to_string());
    Some(WebhookState {
        client: Arc::new(twilight_http::client::ClientBuilder::new().build()),
        marker,
        token,
    })
}
