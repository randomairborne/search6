use axum::extract::{Query, State};
use axum::response::Redirect;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthorizationCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, Scope, TokenResponse,
};
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
    state
        .redis
        .get()
        .await?
        .set_ex(format!("csrf.token:{}", csrf_token.secret()), pkce_verifier.secret(), 600)
        .await?;
    Ok(Redirect::to(auth_url.as_str()))
}

pub async fn set_id(
    State(state): State<AppState>,
    Query(query): Query<SetIdQuery>,
) -> Result<Redirect, Error> {
    let pkce_secret = state
        .redis
        .get()
        .await?
        .get_del::<&str, Option<String>>(&query.state)
        .await?
        .ok_or(Error::InvalidState)?;
    let pkce_verifier = PkceCodeVerifier::new(pkce_secret);
    let token_result = state
        .oauth
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
            state.oauth.revoke_token(rt.into()).ok();
        }
        state
            .oauth
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
