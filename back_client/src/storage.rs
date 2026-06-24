use std::{str::FromStr, time::Duration};

use chrono::{SecondsFormat, Utc};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    QueryBuilder, Row, Sqlite, SqlitePool,
};
use tokio::sync::{mpsc, oneshot, watch};

use crate::{
    error::{LaboriError, Result},
    model::{MeasurementMode, Sample, SessionEvent, SessionSummary},
};

#[derive(Debug)]
pub enum StorageMessage {
    Begin {
        mode: MeasurementMode,
        gate_seconds: f64,
        period_seconds: Option<f64>,
        channels: Vec<u8>,
        reply: oneshot::Sender<Result<i64>>,
    },
    Sample(Sample),
    Event {
        session_id: i64,
        at_sequence: i64,
        kind: &'static str,
        message: String,
    },
    Finish {
        session_id: i64,
        state: &'static str,
        sample_count: u64,
        error: Option<String>,
        reply: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<()>>,
    },
}

#[derive(Clone)]
pub struct StorageHandle {
    tx: mpsc::Sender<StorageMessage>,
    health: watch::Receiver<Option<String>>,
}

impl StorageHandle {
    pub async fn begin(
        &self,
        mode: MeasurementMode,
        gate_seconds: f64,
        period_seconds: Option<f64>,
        channels: Vec<u8>,
    ) -> Result<i64> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(StorageMessage::Begin {
                mode,
                gate_seconds,
                period_seconds,
                channels,
                reply: reply_tx,
            })
            .await
            .map_err(|_| LaboriError::ChannelClosed("storage writer"))?;
        reply_rx
            .await
            .map_err(|_| LaboriError::ChannelClosed("storage reply"))?
    }

    pub fn try_sample(&self, sample: Sample) -> Result<()> {
        self.tx
            .try_send(StorageMessage::Sample(sample))
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => LaboriError::StorageOverrun,
                mpsc::error::TrySendError::Closed(_) => {
                    LaboriError::ChannelClosed("storage writer")
                }
            })
    }

    pub fn try_event(
        &self,
        session_id: i64,
        at_sequence: i64,
        kind: &'static str,
        message: String,
    ) -> Result<()> {
        self.tx
            .try_send(StorageMessage::Event {
                session_id,
                at_sequence,
                kind,
                message,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => LaboriError::StorageOverrun,
                mpsc::error::TrySendError::Closed(_) => {
                    LaboriError::ChannelClosed("storage writer")
                }
            })
    }

    pub async fn finish(
        &self,
        session_id: i64,
        state: &'static str,
        sample_count: u64,
        error: Option<String>,
    ) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(StorageMessage::Finish {
                session_id,
                state,
                sample_count,
                error,
                reply: reply_tx,
            })
            .await
            .map_err(|_| LaboriError::ChannelClosed("storage writer"))?;
        reply_rx
            .await
            .map_err(|_| LaboriError::ChannelClosed("storage reply"))?
    }

    pub fn health(&self) -> watch::Receiver<Option<String>> {
        self.health.clone()
    }

    pub async fn shutdown(&self) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(StorageMessage::Shutdown { reply: reply_tx })
            .await
            .map_err(|_| LaboriError::ChannelClosed("storage writer"))?;
        reply_rx
            .await
            .map_err(|_| LaboriError::ChannelClosed("storage reply"))?
    }
}

pub async fn open(
    database_path: &str,
    queue_capacity: usize,
    batch_size: usize,
    flush_interval: Duration,
) -> Result<(SqlitePool, StorageHandle)> {
    let options = SqliteConnectOptions::from_str(database_path)
        .map_err(|error| LaboriError::Config(error.to_string()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    initialize(&pool).await?;
    recover_interrupted_sessions(&pool).await?;

    let (tx, rx) = mpsc::channel(queue_capacity);
    let (health_tx, health_rx) = watch::channel(None);
    tokio::spawn(writer(
        pool.clone(),
        rx,
        health_tx,
        batch_size,
        flush_interval,
    ));
    Ok((
        pool,
        StorageHandle {
            tx,
            health: health_rx,
        },
    ))
}

async fn initialize(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sessions (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at       TEXT NOT NULL,
            ended_at         TEXT,
            mode             TEXT NOT NULL,
            gate_seconds     REAL NOT NULL,
            period_seconds   REAL,
            channels         TEXT NOT NULL,
            state            TEXT NOT NULL,
            sample_count     INTEGER NOT NULL DEFAULT 0,
            error            TEXT
        )",
    )
    .execute(pool)
    .await?;
    ensure_session_columns(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS samples (
            session_id INTEGER NOT NULL,
            sequence   INTEGER NOT NULL,
            channel    INTEGER NOT NULL,
            started_ns INTEGER NOT NULL,
            ended_ns   INTEGER NOT NULL,
            value      REAL NOT NULL,
            PRIMARY KEY (session_id, sequence),
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        ) WITHOUT ROWID",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS samples_session_channel_sequence
         ON samples(session_id, channel, sequence)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS session_events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id  INTEGER NOT NULL,
            created_at  TEXT NOT NULL,
            at_sequence INTEGER NOT NULL,
            kind        TEXT NOT NULL,
            message     TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn ensure_session_columns(pool: &SqlitePool) -> Result<()> {
    let columns = sqlx::query("PRAGMA table_info(sessions)")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();

    if !columns.iter().any(|name| name == "gate_seconds") {
        sqlx::query("ALTER TABLE sessions ADD COLUMN gate_seconds REAL NOT NULL DEFAULT 0.001")
            .execute(pool)
            .await?;
        if columns.iter().any(|name| name == "interval_seconds") {
            sqlx::query("UPDATE sessions SET gate_seconds = interval_seconds")
                .execute(pool)
                .await?;
        }
    }
    if !columns.iter().any(|name| name == "period_seconds") {
        sqlx::query("ALTER TABLE sessions ADD COLUMN period_seconds REAL")
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn recover_interrupted_sessions(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        "UPDATE sessions
         SET state = 'interrupted',
             ended_at = ?,
             sample_count = (
                 SELECT COUNT(*) FROM samples WHERE samples.session_id = sessions.id
             ),
             error = COALESCE(error, 'labori stopped before the session was finalized')
         WHERE state = 'running'",
    )
    .bind(now())
    .execute(pool)
    .await?;
    Ok(())
}

async fn writer(
    pool: SqlitePool,
    mut rx: mpsc::Receiver<StorageMessage>,
    health: watch::Sender<Option<String>>,
    batch_size: usize,
    flush_interval: Duration,
) {
    let mut samples = Vec::with_capacity(batch_size);
    let mut ticker = tokio::time::interval(flush_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;
            message = rx.recv() => {
                let Some(message) = message else { break };
                let result = match message {
                    StorageMessage::Sample(sample) => {
                        samples.push(sample);
                        if samples.len() >= batch_size {
                            flush_samples(&pool, &mut samples).await
                        } else {
                            Ok(())
                        }
                    }
                    StorageMessage::Begin { mode, gate_seconds, period_seconds, channels, reply } => {
                        let result = async {
                            flush_samples(&pool, &mut samples).await?;
                            begin_session(&pool, mode, gate_seconds, period_seconds, channels).await
                        }.await;
                        let failed = result.as_ref().err().map(ToString::to_string);
                        let _ = reply.send(result);
                        if let Some(error) = failed { Err(LaboriError::Config(error)) } else { Ok(()) }
                    }
                    StorageMessage::Event { session_id, at_sequence, kind, message } => {
                        async {
                            flush_samples(&pool, &mut samples).await?;
                            insert_event(&pool, session_id, at_sequence, kind, &message).await
                        }.await
                    }
                    StorageMessage::Finish { session_id, state, sample_count, error, reply } => {
                        let result = async {
                            flush_samples(&pool, &mut samples).await?;
                            finish_session(&pool, session_id, state, sample_count, error).await
                        }.await;
                        let failed = result.as_ref().err().map(ToString::to_string);
                        let _ = reply.send(result);
                        if let Some(error) = failed { Err(LaboriError::Config(error)) } else { Ok(()) }
                    }
                    StorageMessage::Shutdown { reply } => {
                        let result = flush_samples(&pool, &mut samples).await;
                        let failed = result.as_ref().err().map(ToString::to_string);
                        let _ = reply.send(result);
                        if let Some(error) = failed {
                            Err(LaboriError::Config(error))
                        } else {
                            break;
                        }
                    }
                };
                if let Err(error) = result {
                    let message = error.to_string();
                    let _ = health.send(Some(message.clone()));
                    tracing::error!(%message, "storage writer failed");
                    break;
                }
            }
            _ = ticker.tick(), if !samples.is_empty() => {
                if let Err(error) = flush_samples(&pool, &mut samples).await {
                    let message = error.to_string();
                    let _ = health.send(Some(message.clone()));
                    tracing::error!(%message, "storage writer failed");
                    break;
                }
            }
        }
    }
}

async fn flush_samples(pool: &SqlitePool, samples: &mut Vec<Sample>) -> Result<()> {
    if samples.is_empty() {
        return Ok(());
    }
    let mut transaction = pool.begin().await?;
    let mut builder = QueryBuilder::<Sqlite>::new(
        "INSERT INTO samples
         (session_id, sequence, channel, started_ns, ended_ns, value) ",
    );
    builder.push_values(samples.iter(), |mut row, sample| {
        row.push_bind(sample.session_id)
            .push_bind(sample.sequence)
            .push_bind(sample.channel)
            .push_bind(sample.started_ns)
            .push_bind(sample.ended_ns)
            .push_bind(sample.value);
    });
    builder.build().execute(&mut *transaction).await?;
    transaction.commit().await?;
    samples.clear();
    Ok(())
}

async fn begin_session(
    pool: &SqlitePool,
    mode: MeasurementMode,
    gate_seconds: f64,
    period_seconds: Option<f64>,
    channels: Vec<u8>,
) -> Result<i64> {
    let channels = channels
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let result = sqlx::query(
        "INSERT INTO sessions
         (started_at, mode, gate_seconds, period_seconds, channels, state)
         VALUES (?, ?, ?, ?, ?, 'running')",
    )
    .bind(now())
    .bind(mode.as_str())
    .bind(gate_seconds)
    .bind(period_seconds)
    .bind(channels)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

async fn insert_event(
    pool: &SqlitePool,
    session_id: i64,
    at_sequence: i64,
    kind: &str,
    message: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO session_events
         (session_id, created_at, at_sequence, kind, message)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(session_id)
    .bind(now())
    .bind(at_sequence)
    .bind(kind)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

async fn finish_session(
    pool: &SqlitePool,
    session_id: i64,
    state: &str,
    sample_count: u64,
    error: Option<String>,
) -> Result<()> {
    sqlx::query(
        "UPDATE sessions
         SET ended_at = ?, state = ?, sample_count = ?, error = ?
         WHERE id = ?",
    )
    .bind(now())
    .bind(state)
    .bind(sample_count as i64)
    .bind(error)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_sessions(pool: &SqlitePool, mode: Option<&str>) -> Result<Vec<SessionSummary>> {
    let rows = if mode == Some("single") {
        sqlx::query_as::<_, SessionSummary>(
            "SELECT id, started_at, ended_at, mode, gate_seconds, period_seconds, channels,
                    state, sample_count, error
             FROM sessions
             WHERE mode IN ('single_log', 'single_direct', 'single')
             ORDER BY id DESC LIMIT 1000",
        )
        .fetch_all(pool)
        .await?
    } else if let Some(mode) = mode {
        sqlx::query_as::<_, SessionSummary>(
            "SELECT id, started_at, ended_at, mode, gate_seconds, period_seconds, channels,
                    state, sample_count, error
             FROM sessions WHERE mode = ? ORDER BY id DESC LIMIT 1000",
        )
        .bind(mode)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, SessionSummary>(
            "SELECT id, started_at, ended_at, mode, gate_seconds, period_seconds, channels,
                    state, sample_count, error
             FROM sessions ORDER BY id DESC LIMIT 1000",
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

pub async fn read_samples(
    pool: &SqlitePool,
    session_id: i64,
    after_sequence: i64,
    limit: i64,
) -> Result<Vec<Sample>> {
    Ok(sqlx::query_as::<_, Sample>(
        "SELECT session_id, sequence, channel, started_ns, ended_ns, value
         FROM samples
         WHERE session_id = ? AND sequence > ?
         ORDER BY sequence LIMIT ?",
    )
    .bind(session_id)
    .bind(after_sequence)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn read_events(pool: &SqlitePool, session_id: i64) -> Result<Vec<SessionEvent>> {
    Ok(sqlx::query_as::<_, SessionEvent>(
        "SELECT id, session_id, created_at, at_sequence, kind, message
         FROM session_events
         WHERE session_id = ?
         ORDER BY id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?)
}

pub async fn delete_session(pool: &SqlitePool, session_id: i64) -> Result<()> {
    let result = sqlx::query("DELETE FROM sessions WHERE id = ? AND state != 'running'")
        .bind(session_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(LaboriError::Invalid(
            "session does not exist or is still running".into(),
        ));
    }
    Ok(())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persists_session_and_ordered_samples() {
        let path = std::env::temp_dir().join(format!(
            "labori-test-{}-{}.db",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let path_string = path.to_string_lossy().into_owned();
        let (pool, storage) = open(&path_string, 128, 8, Duration::from_millis(10))
            .await
            .unwrap();
        let session_id = storage
            .begin(MeasurementMode::SingleDirect, 0.001, Some(0.01), Vec::new())
            .await
            .unwrap();
        for sequence in 0..20 {
            storage
                .try_sample(Sample {
                    session_id,
                    sequence,
                    channel: 0,
                    started_ns: sequence * 1_000_000,
                    ended_ns: (sequence + 1) * 1_000_000,
                    value: sequence as f64,
                })
                .unwrap();
        }
        storage
            .finish(session_id, "completed", 20, None)
            .await
            .unwrap();

        let samples = read_samples(&pool, session_id, -1, 100).await.unwrap();
        assert_eq!(samples.len(), 20);
        assert_eq!(samples[19].sequence, 19);
        storage.shutdown().await.unwrap();
        pool.close().await;

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}
