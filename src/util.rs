use base64::Engine;
use redis::AsyncCommands;

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
            "https://cdn.discordapp.com/embed/avatars/{}/{}.png",
            id,
            discrim.parse::<u16>().unwrap_or(1) % 5
        )
    };
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
