use crate::{util::WebhookState, AppState, Error, Player, Players, User};
use mee6::LevelInfo;
use redis::AsyncCommands;
use std::collections::HashMap;
use twilight_model::http::attachment::Attachment;
use twilight_util::builder::embed::{EmbedBuilder, ImageSource};

const PAGE_KEY: &str = "sync:page";
const RANK_KEY: &str = "sync:rank";

#[allow(clippy::module_name_repetitions)]
pub async fn reload_loop(state: AppState) {
    let mut timer = tokio::time::interval(std::time::Duration::from_secs(3));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut redis = state.redis.get().await.unwrap();
    let _: () = redis.set_nx(RANK_KEY, 1).await.unwrap();
    drop(redis);
    loop {
        timer.tick().await;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = get_page(state.clone()).await {
                error!("{e:?}");
            }
        });
    }
}

async fn get_page(state: AppState) -> Result<(), Error> {
    let mut redis = state.redis.get().await?;
    let page: i64 = redis.incr(PAGE_KEY, 1).await?;
    trace!("Fetching page {page}");
    let page = page - 1;
    let mut rank: i64 = redis.get(RANK_KEY).await?;
    let resp = state
        .http
        .get(format!(
            "https://mee6.xyz/api/plugins/levels/leaderboard/{}",
            state.guild_id
        ))
        .query(&[("limit", 1000), ("page", page)])
        .send()
        .await?;
    let players: Players = resp.json().await?;
    let mut serialized_users: Vec<(String, String)> = Vec::with_capacity(2000);
    let mut user_data: HashMap<u64, User> = HashMap::with_capacity(1000);
    for player in players.players {
        match player_to_user(state.clone(), player, rank).await {
            Ok(user) => {
                let Ok(user_string) = serde_json::to_string(&user) else {
                    error!("Failed to serialize user struct");
                    continue;
                };
                serialized_users.push((
                    format!("user.name:{}#{}", user.username, user.discriminator),
                    user.id.to_string(),
                ));
                serialized_users.push((format!("user.id:{}", user.id), user_string));
                user_data.insert(user.id, user);
                rank += 1;
            }
            Err(e) => {
                error!("{e:?}");
                continue;
            }
        };
    }
    if let Err(e) = redis.incr::<_, _, ()>(RANK_KEY, user_data.len()).await {
        error!("{e:?}");
    }
    if let Some(webhook) = state.webhook.clone() {
        let mut user_keys: Vec<String> = Vec::with_capacity(user_data.len());
        for key in user_data.keys() {
            user_keys.push(format!("user.id:{key}"));
        }
        if let Ok(old_users) = redis.mget::<Vec<String>, Vec<String>>(user_keys).await {
            'userchecker: for string_user in old_users {
                let Ok(old_user) = serde_json::from_str::<User>(&string_user) else {
                    warn!("user failed to deserialize");
                    continue 'userchecker;
                };
                let Some(new_user) = user_data.remove(&old_user.id) else {
                    warn!("Webhook user not in user data");
                    continue 'userchecker;
                };
                let old_user_level = LevelInfo::new(old_user.xp).level();
                let new_user_level = LevelInfo::new(new_user.xp).level();
                if new_user_level >= 5 && old_user_level < 5 {
                    let state = state.clone();
                    let whstate = webhook.clone();
                    tokio::spawn(async move {
                        if let Err(e) = send_hook(&state, &whstate, new_user, new_user_level).await
                        {
                            error!("{e:?}");
                        }
                    });
                }
            }
        }
    }
    redis.mset::<String, String, ()>(&serialized_users).await?;
    Ok(())
}

async fn player_to_user(state: AppState, player: Player, rank: i64) -> Result<User, Error> {
    let mut redis = state.redis.get().await?;
    if player.xp < 100 {
        redis.mset(&[(PAGE_KEY, 0), (RANK_KEY, 1)]).await?;
    }
    let id = player.id.parse::<u64>()?;
    let last_updated = Some(chrono::offset::Utc::now().timestamp_millis());
    let user = User {
        xp: player.xp,
        id,
        username: player.username,
        discriminator: player.discriminator,
        avatar: player.avatar,
        message_count: player.message_count,
        rank,
        last_updated,
    };
    Ok(user)
}

async fn send_hook(
    state: &AppState,
    webhook: &WebhookState,
    user: User,
    level: u64,
) -> Result<(), Error> {
    let request = format!("{0}/card?id={1} <@{1}>", &*state.root_url, user.id);
    let embed = EmbedBuilder::new()
        .image(ImageSource::attachment("card.png")?)
        .thumbnail(ImageSource::url(format!(
            "{}/search6.png",
            &*state.root_url
        ))?)
        .description(format!(
            "User {}#{} (<@{}>) has reached level {}```{}```",
            user.username, user.discriminator, user.id, level, request
        ))
        .build();
    let card_svg = crate::util::user_context(state, user).await?;
    let card_raster = state.svg.render(card_svg).await?;
    let card = Attachment {
        description: None,
        file: card_raster,
        filename: "card.png".to_string(),
        id: 0,
    };
    let mut hook_builder = webhook
        .client
        .execute_webhook(webhook.marker, &webhook.token)
        .username("search6 notifier")?
        .avatar_url("https://search6.valk.sh/mee6_bad.png");
    if let Some(thread_id) = webhook.thread {
        hook_builder = hook_builder.thread_id(thread_id);
    };
    hook_builder
        .content(&request)?
        .attachments(&[card])?
        .embeds(&[embed])?
        .await?;
    Ok(())
}
