use axum::{
    extract::{Query, State},
    response::Html,
};

use crate::{AppState, Error};

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
        rank: user.rank,
        name: user.username.clone(),
        discriminator: user.discriminator.clone(),
        percentage: (level_info.percentage() * 100.0).round() as u64,
        current: level_info.xp(),
        needed: mee6::xp_needed_for_level(level_info.level() + 1),
        toy: None,
        avatar: crate::util::get_avatar(&state, user).await?,
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

#[allow(clippy::unused_async)]
pub async fn logo() -> ([(&'static str, &'static str); 1], &'static [u8]) {
    (
        [("Content-Type", "image/png")],
        include_bytes!("mee6_bad.png"),
    )
}

#[allow(clippy::unused_async)]
pub async fn style() -> ([(&'static str, &'static str); 1], &'static str) {
    ([("Content-Type", "text/css")], include_str!("style.css"))
}