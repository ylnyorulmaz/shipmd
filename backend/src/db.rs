use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS columns (
    id        TEXT PRIMARY KEY,
    title     TEXT NOT NULL,
    position  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cards (
    id          TEXT PRIMARY KEY,
    column_id   TEXT NOT NULL REFERENCES columns(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    description TEXT,
    tag         TEXT NOT NULL CHECK (tag IN ('build', 'sell', 'ops')),
    assignee    TEXT NOT NULL CHECK (assignee IN ('human', 'agent')),
    position    REAL NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cards_column_position ON cards(column_id, position);
"#;

pub const COLUMNS: [(&str, &str); 4] = [
    ("backlog", "Backlog"),
    ("build", "Build"),
    ("sell", "Sell"),
    ("done", "Done"),
];

/// (column, title, description, tag, assignee)
const SEED_CARDS: [(&str, &str, &str, &str, &str); 5] = [
    (
        "backlog",
        "Stripe usage-based billing for Pro tier",
        "Meter API calls per workspace and push usage records nightly.\n\n- [ ] Add `usage_events` table\n- [ ] Nightly cron → Stripe Meter API\n- [ ] Invoice preview on settings page",
        "build",
        "agent",
    ),
    (
        "backlog",
        "Cold email 30 indie SaaS founders",
        "Pull the list from the **Indie Hackers** \"$1k MRR\" thread. Personal first line, one ask: a 15‑min call.",
        "sell",
        "human",
    ),
    (
        "build",
        "Webhook retries with exponential backoff",
        "Retry 5xx / timeouts at 1m, 5m, 30m, 2h, 12h. Dead-letter after that and surface in the dashboard.",
        "build",
        "agent",
    ),
    (
        "sell",
        "Record 2‑min Loom demo for landing page",
        "Show: connect repo → first deploy → share link. Keep it under **120 seconds**.",
        "sell",
        "human",
    ),
    (
        "done",
        "Launch on Product Hunt",
        "Hit #4 product of the day — **212 upvotes**, 38 signups, 3 paid. Reply to every comment within 24h.",
        "sell",
        "human",
    ),
];

pub async fn connect(path: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;

    sqlx::raw_sql(SCHEMA).execute(&pool).await?;
    seed(&pool).await?;
    Ok(pool)
}

async fn seed(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM columns")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    for (i, (id, title)) in COLUMNS.iter().enumerate() {
        sqlx::query("INSERT INTO columns (id, title, position) VALUES (?, ?, ?)")
            .bind(id)
            .bind(title)
            .bind(i as i64)
            .execute(&mut *tx)
            .await?;
    }

    let now = crate::now();
    for (i, (col, title, desc, tag, assignee)) in SEED_CARDS.iter().enumerate() {
        sqlx::query(
            "INSERT INTO cards (id, column_id, title, description, tag, assignee, position, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(col)
        .bind(title)
        .bind(desc)
        .bind(tag)
        .bind(assignee)
        .bind(((i + 1) * 1024) as f64)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!("seeded fresh board");
    Ok(())
}
