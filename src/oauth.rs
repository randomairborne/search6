use axum::extract::{Query, State};
use axum::response::Redirect;
use oauth2::reqwest::async_http_client;
use oauth2::{AuthorizationCode, CsrfToken, PkceCodeChallenge, Scope};
use redis::AsyncCommands;

use crate::{AppState, Error};

pub async fn redirect(State(state): State<AppState>) -> Result<Redirect, Error> {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_token) = state
        .oauth
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("identify".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();
    state.redis.get().await?.set_ex();
    Redirect::to(auth_url.as_str())
}

pub async fn set_id(
    State(state): State<AppState>,
    Query(query): Query<SetIdQuery>,
) -> Result<Redirect, Error> {
    let pkce_verifier = state
        .tokens
        .write()
        .await
        .remove(&query.state)
        .ok_or(Error::InvalidState)?;
    let token_result = state
        .client
        .exchange_code(AuthorizationCode::new(query.code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|_| Error::CodeExchangeFailed)?;
    let me: twilight_model::user::CurrentUser = state
        .http
        .get("https://discord.com/api/v10/users/@me")
        .bearer_auth(token_result.access_token().secret())
        .send()
        .await?
        .json()
        .await?;
    tokio::spawn(async move {
        if let Some(rt) = token_result.refresh_token() {
            state.client.revoke_token(rt.into()).ok();
        }
        state
            .client
            .revoke_token(token_result.access_token().into())
            .ok();
    });
    Ok(Redirect::to(&format!(
        "/?id={}&userexists={}",
        me.id.get(),
        true
    )))
}

#[derive(serde::Deserialize)]
pub struct SetIdQuery {
    code: String,
    state: String,
}
