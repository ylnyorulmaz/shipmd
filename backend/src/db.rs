use std::{collections::HashSet, time::Duration};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous},
    Acquire,
};

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
    assignee    TEXT NOT NULL DEFAULT 'human',
    status      TEXT NOT NULL DEFAULT 'todo',
    priority    INTEGER NOT NULL DEFAULT 0,
    pr_url      TEXT,
    repo        TEXT,
    labels      TEXT NOT NULL DEFAULT '[]',
    position    REAL NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cards_column_position ON cards(column_id, position);

CREATE TABLE IF NOT EXISTS card_comments (
    id         TEXT PRIMARY KEY,
    card_id    TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
    author     TEXT NOT NULL,
    comment    TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_card_comments_card_created ON card_comments(card_id, created_at);
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
    migrate(&pool).await?;
    seed(&pool).await?;
    Ok(pool)
}

async fn cards_columns(pool: &SqlitePool) -> Result<HashSet<String>, sqlx::Error> {
    let rows: Vec<(i64, String, String, i64, Option<String>, i64)> =
        sqlx::query_as("PRAGMA table_info(cards)")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(_, name, _, _, _, _)| name).collect())
}

fn has_restrictive_assignee_check(create_sql: &str) -> bool {
    let normalized: String = create_sql
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    normalized.contains("check (assignee in") || normalized.contains("check(assignee in")
}

/// Idempotent, backward-compatible migration for existing `board.db` files.
/// Ensures `assignee` accepts arbitrary agent identifiers (e.g. "antigravity", "hermes", "human")
/// and adds `status`, `priority`, and `pr_url` columns when missing.
pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let create_sql: Option<(Option<String>,)> =
        sqlx::query_as("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'cards'")
            .fetch_optional(pool)
            .await?;
    let existing_cols = cards_columns(pool).await?;

    if create_sql
        .and_then(|(s,)| s)
        .as_deref()
        .is_some_and(has_restrictive_assignee_check)
    {
        let mut conn = pool.acquire().await?;
        sqlx::query("PRAGMA foreign_keys = OFF").execute(&mut *conn).await?;

        let migration_res: Result<(), sqlx::Error> = async {
            let mut tx = conn.begin().await?;
            sqlx::query(
                "CREATE TABLE cards_migrated (
                    id          TEXT PRIMARY KEY,
                    column_id   TEXT NOT NULL REFERENCES columns(id) ON DELETE CASCADE,
                    title       TEXT NOT NULL,
                    description TEXT,
                    tag         TEXT NOT NULL CHECK (tag IN ('build', 'sell', 'ops')),
                    assignee    TEXT NOT NULL DEFAULT 'human',
                    status      TEXT NOT NULL DEFAULT 'todo',
                    priority    INTEGER NOT NULL DEFAULT 0,
                    pr_url      TEXT,
                    repo        TEXT,
                    labels      TEXT NOT NULL DEFAULT '[]',
                    position    REAL NOT NULL,
                    updated_at  TEXT NOT NULL
                )",
            )
            .execute(&mut *tx)
            .await?;

            let assignee_expr = if existing_cols.contains("assignee") {
                "COALESCE(NULLIF(TRIM(assignee), ''), 'human')"
            } else {
                "'human'"
            };
            let status_fallback = "CASE column_id
                WHEN 'backlog' THEN 'todo'
                WHEN 'build' THEN 'in_progress'
                WHEN 'sell' THEN 'sell'
                WHEN 'done' THEN 'done'
                ELSE 'todo'
            END";
            let status_expr = if existing_cols.contains("status") {
                format!("COALESCE(NULLIF(TRIM(status), ''), {status_fallback})")
            } else {
                status_fallback.to_string()
            };
            let priority_expr = if existing_cols.contains("priority") {
                "COALESCE(priority, 0)"
            } else {
                "0"
            };
            let pr_url_expr = if existing_cols.contains("pr_url") { "pr_url" } else { "NULL" };
            let repo_expr = if existing_cols.contains("repo") { "repo" } else { "NULL" };
            let labels_expr = if existing_cols.contains("labels") {
                "COALESCE(NULLIF(TRIM(labels), ''), '[]')"
            } else {
                "'[]'"
            };

            let copy_sql = format!(
                "INSERT INTO cards_migrated (id, column_id, title, description, tag, assignee, status, priority, pr_url, repo, labels, position, updated_at)
                 SELECT id, column_id, title, description, tag, {assignee_expr}, {status_expr}, {priority_expr}, {pr_url_expr}, {repo_expr}, {labels_expr}, position, updated_at
                 FROM cards"
            );
            sqlx::query(&copy_sql).execute(&mut *tx).await?;
            sqlx::query("DROP TABLE cards").execute(&mut *tx).await?;
            sqlx::query("ALTER TABLE cards_migrated RENAME TO cards").execute(&mut *tx).await?;
            sqlx::query("CREATE INDEX IF NOT EXISTS idx_cards_column_position ON cards(column_id, position)")
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(())
        }
        .await;

        let _ = sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *conn).await;
        migration_res?;
        tracing::info!("migrated cards table schema (relaxed assignee constraint + task fields)");
    } else {
        if !existing_cols.contains("assignee") {
            sqlx::query("ALTER TABLE cards ADD COLUMN assignee TEXT NOT NULL DEFAULT 'human'")
                .execute(pool)
                .await?;
            tracing::info!("migrated cards table: added assignee column");
        }
        if !existing_cols.contains("status") {
            sqlx::query("ALTER TABLE cards ADD COLUMN status TEXT NOT NULL DEFAULT 'todo'")
                .execute(pool)
                .await?;
            sqlx::query(
                "UPDATE cards SET status = CASE column_id
                    WHEN 'backlog' THEN 'todo'
                    WHEN 'build' THEN 'in_progress'
                    WHEN 'sell' THEN 'sell'
                    WHEN 'done' THEN 'done'
                    ELSE 'todo'
                END",
            )
            .execute(pool)
            .await?;
            tracing::info!("migrated cards table: added status column");
        }
        if !existing_cols.contains("priority") {
            sqlx::query("ALTER TABLE cards ADD COLUMN priority INTEGER NOT NULL DEFAULT 0")
                .execute(pool)
                .await?;
            tracing::info!("migrated cards table: added priority column");
        }
        if !existing_cols.contains("pr_url") {
            sqlx::query("ALTER TABLE cards ADD COLUMN pr_url TEXT")
                .execute(pool)
                .await?;
            tracing::info!("migrated cards table: added pr_url column");
        }
        if !existing_cols.contains("repo") {
            sqlx::query("ALTER TABLE cards ADD COLUMN repo TEXT")
                .execute(pool)
                .await?;
            tracing::info!("migrated cards table: added repo column");
        }
        if !existing_cols.contains("labels") {
            sqlx::query("ALTER TABLE cards ADD COLUMN labels TEXT NOT NULL DEFAULT '[]'")
                .execute(pool)
                .await?;
            tracing::info!("migrated cards table: added labels column");
        }
    }

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_cards_priority ON cards(priority DESC, position ASC)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_cards_repo ON cards(repo)")
        .execute(pool)
        .await?;
    Ok(())
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
        let status = match *col {
            "backlog" => "todo",
            "build" => "in_progress",
            "sell" => "sell",
            "done" => "done",
            _ => "todo",
        };
        sqlx::query(
            "INSERT INTO cards (id, column_id, title, description, tag, assignee, status, priority, pr_url, repo, labels, position, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, NULL, NULL, '[]', ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(col)
        .bind(title)
        .bind(desc)
        .bind(tag)
        .bind(assignee)
        .bind(status)
        .bind(((i + 1) * 1024) as f64)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!("seeded fresh board");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrates_legacy_cards_table_preserving_data() {
        let path = format!("/tmp/shipmd_migrate_test_{}.db", uuid::Uuid::new_v4());
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();

        sqlx::raw_sql(
            r#"
            CREATE TABLE columns (id TEXT PRIMARY KEY, title TEXT NOT NULL, position INTEGER NOT NULL);
            CREATE TABLE cards (
                id          TEXT PRIMARY KEY,
                column_id   TEXT NOT NULL REFERENCES columns(id) ON DELETE CASCADE,
                title       TEXT NOT NULL,
                description TEXT,
                tag         TEXT NOT NULL CHECK (tag IN ('build', 'sell', 'ops')),
                assignee    TEXT NOT NULL CHECK (assignee IN ('human', 'agent')),
                position    REAL NOT NULL,
                updated_at  TEXT NOT NULL
            );
            INSERT INTO columns (id, title, position) VALUES ('backlog', 'Backlog', 0), ('build', 'Build', 1);
            INSERT INTO cards (id, column_id, title, description, tag, assignee, position, updated_at)
            VALUES ('c1', 'build', 'Old task', 'Details', 'build', 'agent', 1024.0, '2026-01-01T00:00:00Z');
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let migrated = connect(&path).await.unwrap();

        let (status, priority, pr_url, repo, labels): (String, i64, Option<String>, Option<String>, String) =
            sqlx::query_as("SELECT status, priority, pr_url, repo, labels FROM cards WHERE id = 'c1'")
                .fetch_one(&migrated)
                .await
                .unwrap();
        assert_eq!(status, "in_progress");
        assert_eq!(priority, 0);
        assert!(pr_url.is_none());
        assert!(repo.is_none());
        assert_eq!(labels, "[]");

        sqlx::query(
            "INSERT INTO cards (id, column_id, title, description, tag, assignee, status, priority, pr_url, repo, labels, position, updated_at)
             VALUES ('c2', 'backlog', 'Hermes task', NULL, 'build', 'hermes', 'todo', 5, 'https://github.com/org/repo/pull/1', 'shipmd', '[\"frontend\",\"bug\"]', 2048.0, '2026-01-01T00:00:00Z')",
        )
        .execute(&migrated)
        .await
        .unwrap();

        let (assignee, repo2, labels2): (String, Option<String>, String) =
            sqlx::query_as("SELECT assignee, repo, labels FROM cards WHERE id = 'c2'")
                .fetch_one(&migrated)
                .await
                .unwrap();
        assert_eq!(assignee, "hermes");
        assert_eq!(repo2.as_deref(), Some("shipmd"));
        assert_eq!(labels2, "[\"frontend\",\"bug\"]");

        migrated.close().await;
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{path}-wal"));
        let _ = std::fs::remove_file(format!("{path}-shm"));
    }

    #[tokio::test]
    async fn migrates_existing_table_missing_repo_and_labels() {
        let path = format!("/tmp/shipmd_migrate_repo_labels_{}.db", uuid::Uuid::new_v4());
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();

        sqlx::raw_sql(
            r#"
            CREATE TABLE columns (id TEXT PRIMARY KEY, title TEXT NOT NULL, position INTEGER NOT NULL);
            CREATE TABLE cards (
                id          TEXT PRIMARY KEY,
                column_id   TEXT NOT NULL REFERENCES columns(id) ON DELETE CASCADE,
                title       TEXT NOT NULL,
                description TEXT,
                tag         TEXT NOT NULL CHECK (tag IN ('build', 'sell', 'ops')),
                assignee    TEXT NOT NULL DEFAULT 'human',
                status      TEXT NOT NULL DEFAULT 'todo',
                priority    INTEGER NOT NULL DEFAULT 0,
                pr_url      TEXT,
                position    REAL NOT NULL,
                updated_at  TEXT NOT NULL
            );
            INSERT INTO columns (id, title, position) VALUES ('backlog', 'Backlog', 0);
            INSERT INTO cards (id, column_id, title, description, tag, assignee, status, priority, pr_url, position, updated_at)
            VALUES ('c1', 'backlog', 'Existing card', NULL, 'build', 'kimi', 'todo', 8, NULL, 1024.0, '2026-01-01T00:00:00Z');
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let migrated = connect(&path).await.unwrap();
        let (repo, labels): (Option<String>, String) =
            sqlx::query_as("SELECT repo, labels FROM cards WHERE id = 'c1'")
                .fetch_one(&migrated)
                .await
                .unwrap();
        assert!(repo.is_none());
        assert_eq!(labels, "[]");

        migrated.close().await;
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{path}-wal"));
        let _ = std::fs::remove_file(format!("{path}-shm"));
    }
}
