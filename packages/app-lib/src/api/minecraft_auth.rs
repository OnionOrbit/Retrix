use uuid::Uuid;

/// Offline login: create credentials for a username (cracked/offline mode)
#[tracing::instrument]
pub async fn offline_login(username: &str) -> crate::Result<Credentials> {
    use crate::state::Credentials;
    use crate::state::MinecraftProfile;
    use chrono::{Utc, Duration};

    // Generate offline UUID (same as vanilla: UUID v3 with namespace and username)
    // Use UUID v5 (SHA-1) for offline UUID generation, as v3 (MD5) is deprecated and not available in uuid 1.x
    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_DNS, username.as_bytes());

    let profile = MinecraftProfile {
        id: uuid,
        name: username.to_string(),
        skins: vec![],
        capes: vec![],
        fetch_time: None,
    };

    let credentials = Credentials {
        offline_profile: profile,
        access_token: "offline".to_string(),
        refresh_token: "offline".to_string(),
        expires: Utc::now() + Duration::days(3650), // 10 years
        active: true,
    };

    // Save to DB (optional, for account management)
    let state = crate::State::get().await?;
    credentials.upsert(&state.pool).await?;

    Ok(credentials)
}
/// Authentication flow interface

use crate::State;
use crate::state::{Credentials, MinecraftLoginFlow};

#[tracing::instrument]
pub async fn begin_login() -> crate::Result<MinecraftLoginFlow> {
    let state = State::get().await?;

    crate::state::login_begin(&state.pool).await
}

#[tracing::instrument]
pub async fn finish_login(
    code: &str,
    flow: MinecraftLoginFlow,
) -> crate::Result<Credentials> {
    let state = State::get().await?;

    crate::state::login_finish(code, flow, &state.pool).await
}

#[tracing::instrument]
pub async fn get_default_user() -> crate::Result<Option<uuid::Uuid>> {
    let state = State::get().await?;
    let user = Credentials::get_active(&state.pool).await?;
    Ok(user.map(|user| user.offline_profile.id))
}

#[tracing::instrument]
pub async fn set_default_user(user: uuid::Uuid) -> crate::Result<()> {
    let state = State::get().await?;
    let users = Credentials::get_all(&state.pool).await?;
    let (_, mut user) = users.remove(&user).ok_or_else(|| {
        crate::ErrorKind::OtherError(format!(
            "Tried to get nonexistent user with ID {user}"
        ))
        .as_error()
    })?;

    user.active = true;
    user.upsert(&state.pool).await?;

    Ok(())
}

/// Remove a user account from the database
#[tracing::instrument]
pub async fn remove_user(uuid: uuid::Uuid) -> crate::Result<()> {
    let state = State::get().await?;

    let users = Credentials::get_all(&state.pool).await?;

    if let Some((uuid, user)) = users.remove(&uuid) {
        Credentials::remove(uuid, &state.pool).await?;

        if user.active
            && let Some((_, mut user)) = users.into_iter().next()
        {
            user.active = true;
            user.upsert(&state.pool).await?;
        }
    }

    Ok(())
}

/// Get a copy of the list of all user credentials
#[tracing::instrument]
pub async fn users() -> crate::Result<Vec<Credentials>> {
    let state = State::get().await?;
    let users = Credentials::get_all(&state.pool).await?;
    Ok(users.into_iter().map(|x| x.1).collect())
}
