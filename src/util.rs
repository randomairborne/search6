use std::sync::Arc;

use base64::Engine;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
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
        if user_exists {
            id.ok_or(Error::NotLevelFive)?
        } else {
            id.ok_or(Error::UnknownId)?
        }
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

pub async fn get_user_context(
    state: &AppState,
    id: String,
    user_exists: bool,
) -> Result<xpd_rank_card::Context, Error> {
    let user = get_user(state.redis.get().await?, id, user_exists).await?;
    user_context(state, user).await
}

pub async fn user_context(state: &AppState, user: User) -> Result<xpd_rank_card::Context, Error> {
    let level_info = mee6::LevelInfo::new(user.xp);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ctx = xpd_rank_card::Context {
        level: level_info.level(),
        rank: user.rank,
        name: user.username.clone(),
        discriminator: user.discriminator.clone(),
        percentage: (level_info.percentage() * 100.0).round() as u64,
        current: level_info.xp(),
        needed: mee6::xp_needed_for_level(level_info.level() + 1),
        toy: None,
        avatar: crate::util::get_avatar_data(state, &user).await?,
        font: xpd_rank_card::Font::Mojang,
        colors: xpd_rank_card::colors::Colors::default(),
    };
    Ok(ctx)
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

pub fn time_since_epoch(update_epoch_time: i64) -> Option<Duration> {
    let naive_update_time = NaiveDateTime::from_timestamp_millis(update_epoch_time)?;
    let update_time = DateTime::<Utc>::from_utc(naive_update_time, Utc);
    let duration = Utc::now().signed_duration_since(update_time);
    Some(duration)
}

pub fn duration_fmt(duration: Duration) -> String {
    let mut prev_set = false;
    let mut out = String::with_capacity(128);
    let week_count = duration.num_weeks();
    let day_count = duration.num_days() % 7;
    let hour_count = duration.num_hours() % 24;
    let minute_count = duration.num_minutes() % 60;
    let second_count = duration.num_seconds() % 60;

    fmt_unit!(out, week_count, "week", prev_set);
    fmt_unit!(out, day_count, "day", prev_set);
    fmt_unit!(out, hour_count, "hour", prev_set);
    fmt_unit!(out, minute_count, "minute", prev_set);
    fmt_unit!(out, second_count, "second", prev_set);
    out
}

macro_rules! fmt_unit {
    ($out:expr, $num:expr, $unit_name:expr, $pset:expr) => {
        #[allow(unused_assignments)]
        if $num != 0 {
            if $pset {
                $out.push_str(", ");
            }
            $out.push_str(&$num.to_string());
            if $num == 1 {
                $out.push_str(concat!(" ", $unit_name));
            } else {
                $out.push_str(concat!(" ", $unit_name, "s"));
            }
            $pset = true;
        }
    };
}
use fmt_unit;
