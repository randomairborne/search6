use crate::{util::WebhookState, AppState, Error, Players, User};
use mee6::LevelInfo;
use redis::AsyncCommands;
use std::collections::HashMap;
use std::sync::Arc;
use twilight_model::{
    channel::message::AllowedMentions,
    id::{marker::UserMarker, Id},
};
use twilight_util::builder::embed::{EmbedBuilder, ImageSource};

#[allow(clippy::module_name_repetitions)]
pub async fn reload_loop(state: AppState) {
    let mut timer = tokio::time::interval(std::time::Duration::from_secs(3));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut page = 0usize;
    let mut rank = 1i64;
    'update: loop {
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
                error!("{e:?}");
                continue 'update;
            }
        };
        let players: Players = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                error!("{e:?}");
                continue 'update;
            }
        };
        let mut serialized_users: Vec<(String, String)> = Vec::with_capacity(2000);
        let mut user_keys: Vec<String> = Vec::with_capacity(1000);
        let mut user_data: HashMap<u64, User> = HashMap::with_capacity(1000);
        'insert: for player in players.players {
            if player.xp < 100 {
                rank = 1;
                page = 0;
                continue 'update;
            }
            let Ok(id) = player.id.parse::<u64>() else {
                continue 'insert;
            };
            let slug_key = format!("user.slug:{}#{}", player.username, player.discriminator);
            let user = User {
                xp: player.xp,
                id,
                username: player.username,
                discriminator: player.discriminator,
                avatar: player.avatar,
                message_count: player.message_count,
                rank,
            };
            let Ok(data) = serde_json::to_string(&user) else { continue 'insert; };
            serialized_users.push((slug_key, id.to_string()));
            serialized_users.push((format!("user.id:{id}"), data));
            user_keys.push(format!("user.id:{id}"));
            user_data.insert(user.id, user);
            rank += 1;
        }
        let Ok(mut redis) = state.redis.get().await else { continue 'update; };
        if let Some(webhook) = state.webhook.clone() {
            if let Ok(users) = redis.mget::<Vec<String>, Vec<String>>(user_keys).await {
                'userchecker: for string_user in users {
                    let Ok(old_user) = serde_json::from_str::<User>(&string_user) else { continue 'userchecker; };
                    let Some(new_user) = user_data.remove(&old_user.id) else { continue 'userchecker; };
                    let old_user_level = LevelInfo::new(old_user.xp).level();
                    let new_user_level = LevelInfo::new(new_user.xp).level();
                    if new_user_level >= 5 && old_user_level < 5 {
                        let webhook = webhook.clone();
                        let root_url = state.root_url.clone();
                        tokio::spawn(async move {
                            if let Err(e) =
                                send_hook(webhook, root_url, new_user, new_user_level).await
                            {
                                error!("{e:?}");
                            }
                        });
                    }
                }
            }
        }
        if let Err(redis_error) = redis
            .set_multiple::<String, String, ()>(&serialized_users)
            .await
        {
            error!("{redis_error:?}");
            continue 'update;
        };
        page += 1;
    }
}

async fn send_hook(
    state: WebhookState,
    root_url: Arc<String>,
    user: User,
    level: u64,
) -> Result<(), Error> {
    let embed = EmbedBuilder::new()
        .image(ImageSource::url(format!(
            "{}/card?id={}",
            &*root_url, user.id
        ))?)
        .description(format!(
            "User {}#{} (<@{}>) has reached level {}",
            user.username, user.discriminator, user.id, level
        ))
        .build();
    let mut allowedmentions = AllowedMentions::default();
    allowedmentions
        .users
        .push(Id::<UserMarker>::new(187_384_089_228_214_273));
    state
        .client
        .execute_webhook(state.marker, &state.token)
        .username("search6 notifier")?
        .embeds(&[embed])?
        .content(&format!(
            "```https://search6.valk.sh/card?id={} <@{}>```",
            user.id, user.id
        ))?
        .allowed_mentions(Some(&allowedmentions))
        .avatar_url("https://search6.valk.sh/mee6_bad.png")
        .await?;
    Ok(())
}
