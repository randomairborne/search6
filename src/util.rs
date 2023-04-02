use base64::Engine;

use crate::{AppState, Error, User};

pub async fn get_avatar(state: &AppState, user: &User) -> Result<String, Error> {
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
