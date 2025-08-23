use crate::state::DirectoryInfo;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
};
use sqlx::{Pool, Sqlite};
use std::str::FromStr;
use std::time::Duration;

pub(crate) async fn connect() -> crate::Result<Pool<Sqlite>> {
    let settings_dir = DirectoryInfo::get_initial_settings_dir().ok_or(
        crate::ErrorKind::FSError(
            "Could not find valid config dir".to_string(),
        ),
    )?;

    if !settings_dir.exists() {
        crate::util::io::create_dir_all(&settings_dir).await?;
    }

    let uri = format!("sqlite:{}", settings_dir.join("app.db").display());

    let conn_options = SqliteConnectOptions::from_str(&uri)?
        .busy_timeout(Duration::from_secs(30))
        .journal_mode(SqliteJournalMode::Wal)
        .optimize_on_close(true, None)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(100)
        .connect_with(conn_options)
        .await?;

    sqlx::migrate!().run(&pool).await?;

    if let Err(err) = stale_data_cleanup(&pool).await {
        tracing::warn!(
            "Failed to clean up stale data from state database: {err}"
        );
    }

    Ok(pool)
}

/// Cleans up data from the database that is no longer referenced, but must be
/// kept around for a little while to allow users to recover from accidental
/// deletions.
async fn stale_data_cleanup(pool: &Pool<Sqlite>) -> crate::Result<()> {
    let tx = pool.begin().await?;

    // Skins feature removed: cleanup queries no longer applicable

    tx.commit().await?;

    Ok(())
}
