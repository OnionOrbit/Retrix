use crate::state::{CacheBehaviour, CachedEntry};
use crate::util::fetch::{FetchSemaphore, fetch_advanced};
use chrono::{DateTime, Duration, TimeZone, Utc};
use dashmap::DashMap;
use futures::TryStreamExt;
use reqwest::Method;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModrinthCredentials {
    pub session: String,
    pub expires: DateTime<Utc>,
    pub user_id: String,
    pub active: bool,
}

impl ModrinthCredentials {
    pub async fn get_and_refresh(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite> + Copy,
        semaphore: &FetchSemaphore,
    ) -> crate::Result<Option<Self>> {
        let creds = Self::get_active(exec).await?;

        if let Some(mut creds) = creds {
            // Refresh session if it expires in less than 1 hour
            if creds.expires - Utc::now() < Duration::hours(1) {
                #[derive(Deserialize)]
                struct SessionResponse {
                    session: String,
                }
    
                match fetch_advanced(
                    Method::POST,
                    &format!("{}session/refresh", std::env::var("MODRINTH_API_URL").unwrap_or_else(|_| "https://api.modrinth.com/".to_string())),
                    None,
                    None,
                    Some(("Authorization", &*creds.session)),
                    None,
                    semaphore,
                    exec,
                )
                .await
                {
                    Ok(resp) => {
                        if let Ok(session) = serde_json::from_slice::<SessionResponse>(&resp) {
                            creds.session = session.session;
                            creds.expires = Utc::now() + Duration::weeks(2);
                            creds.upsert(exec).await?;
                            Ok(Some(creds))
                        } else {
                            // Failed to parse response
                            Self::remove(&creds.user_id, exec).await?;
                            Ok(None)
                        }
                    },
                    Err(_) => {
                        // Failed to refresh session
                        Self::remove(&creds.user_id, exec).await?;
                        Ok(None)
                    }
                }
            } else {
                Ok(Some(creds))
            }
        } else {
            Ok(None)
        }
    }

    pub async fn get_active(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<Option<Self>> {
        let res = sqlx::query!(
            "
            SELECT
                id, active, session_id, expires
            FROM modrinth_users
            WHERE active = TRUE
            "
        )
        .fetch_optional(exec)
        .await?;

        Ok(res.map(|x| Self {
            session: x.session_id,
            expires: Utc
                .timestamp_opt(x.expires, 0)
                .single()
                .unwrap_or_else(Utc::now),
            user_id: x.id,
            active: x.active == 1,
        }))
    }

    pub async fn get_all(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<DashMap<String, Self>> {
        let res = sqlx::query!(
            "
            SELECT
                id, active, session_id, expires
            FROM modrinth_users
            "
        )
        .fetch(exec)
        .try_fold(DashMap::new(), |acc, x| {
            acc.insert(
                x.id.clone(),
                Self {
                    session: x.session_id,
                    expires: Utc
                        .timestamp_opt(x.expires, 0)
                        .single()
                        .unwrap_or_else(Utc::now),
                    user_id: x.id,
                    active: x.active == 1,
                },
            );

            async move { Ok(acc) }
        })
        .await?;

        Ok(res)
    }

    pub async fn upsert(
        &self,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite> + Copy,
    ) -> crate::Result<()> {
        let expires = self.expires.timestamp();

        if self.active {
            sqlx::query!(
                "
                UPDATE modrinth_users
                SET active = FALSE
                "
            )
            .execute(exec)
            .await?;
        }

        sqlx::query!(
            "
            INSERT INTO modrinth_users (id, active, session_id, expires)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE SET
                active = $2,
                session_id = $3,
                expires = $4
            ",
            self.user_id,
            self.active,
            self.session,
            expires,
        )
        .execute(exec)
        .await?;

        Ok(())
    }

    pub async fn remove(
        user_id: &str,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query!(
            "
            DELETE FROM modrinth_users WHERE id = $1
            ",
            user_id,
        )
        .execute(exec)
        .await?;

        Ok(())
    }

    pub(crate) async fn refresh_all() -> crate::Result<()> {
        let state = crate::State::get().await?;
        let all = Self::get_all(&state.pool).await?;

        let user_ids = all.into_iter().map(|x| x.0).collect::<Vec<_>>();

        CachedEntry::get_user_many(
            &user_ids.iter().map(|x| &**x).collect::<Vec<_>>(),
            Some(CacheBehaviour::Bypass),
            &state.pool,
            &state.fetch_semaphore,
        )
        .await?;

        Ok(())
    }
}

pub fn get_login_url() -> String {
    format!("{}auth/sign-in", std::env::var("MODRINTH_URL").unwrap_or_else(|_| "https://modrinth.com/".to_string()))
}

pub async fn finish_login_flow(
    code: &str,
    semaphore: &FetchSemaphore,
    exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
) -> crate::Result<ModrinthCredentials> {
    // The authorization code actually is the access token, since labrinth doesn't
    // issue separate authorization codes. Therefore, this is equivalent to an
    // implicit OAuth grant flow, and no additional exchanging or finalization is
    // needed. TODO not do this for the reasons outlined at
    // https://oauth.net/2/grant-types/implicit/

    let info = fetch_info(code, semaphore, exec).await?;

    Ok(ModrinthCredentials {
        session: code.to_string(),
        expires: Utc::now() + Duration::weeks(2),
        user_id: info.id,
        active: true,
    })
}

async fn fetch_info(
    token: &str,
    semaphore: &FetchSemaphore,
    exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
) -> crate::Result<crate::state::cache::User> {
    let result = fetch_advanced(
        Method::GET,
        &format!("{}user", std::env::var("MODRINTH_API_URL").unwrap_or_else(|_| "https://api.modrinth.com/".to_string())),
        None,
        None,
        Some(("Authorization", token)),
        None,
        semaphore,
        exec,
    )
    .await?;
    let value = serde_json::from_slice(&result)?;

    Ok(value)
}
