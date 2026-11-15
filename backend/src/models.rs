use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{sqlite::SqliteRow, FromRow, Row};

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Column {
    pub id: String,
    pub title: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub id: String,
    pub column_id: String,
    pub title: String,
    pub description: Option<String>,
    pub tag: String,
    pub assignee: String,
    pub status: String,
    pub priority: i64,
    pub pr_url: Option<String>,
    pub repo: Option<String>,
    pub labels: Vec<String>,
    pub position: f64,
    pub updated_at: String,
}

/// Normalize stored labels (`["frontend","bug"]` JSON array or `"frontend, bug"` CSV).
pub fn parse_stored_labels(raw: Option<&str>) -> Vec<String> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    let items: Vec<String> = if s.starts_with('[') {
        serde_json::from_str(s).unwrap_or_else(|_| vec![s.to_string()])
    } else {
        s.split(',').map(ToString::to_string).collect()
    };
    normalize_label_list(items)
}

/// Strip leading `#`, lowercase, deduplicate, and drop empty entries.
pub fn normalize_label_list<I, S>(items: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = Vec::new();
    for item in items {
        for part in item.as_ref().split(',') {
            let cleaned = part.trim().trim_start_matches('#').trim().to_ascii_lowercase();
            if cleaned.is_empty() || out.contains(&cleaned) {
                continue;
            }
            out.push(cleaned);
        }
    }
    out
}

impl<'r> FromRow<'r, SqliteRow> for Card {
    fn from_row(row: &'r SqliteRow) -> Result<Self, sqlx::Error> {
        let labels_raw: Option<String> = row.try_get("labels")?;
        Ok(Self {
            id: row.try_get("id")?,
            column_id: row.try_get("column_id")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            tag: row.try_get("tag")?,
            assignee: row.try_get("assignee")?,
            status: row.try_get("status")?,
            priority: row.try_get("priority")?,
            pr_url: row.try_get("pr_url")?,
            repo: row.try_get("repo")?,
            labels: parse_stored_labels(labels_raw.as_deref()),
            position: row.try_get("position")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CardComment {
    pub id: String,
    pub card_id: String,
    pub author: String,
    pub comment: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub links: Vec<ProjectLink>,
    pub updated_at: String,
}

pub fn parse_stored_links(raw: Option<&str>) -> Vec<ProjectLink> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<ProjectLink>>(s).unwrap_or_default()
}

impl<'r> FromRow<'r, SqliteRow> for Project {
    fn from_row(row: &'r SqliteRow) -> Result<Self, sqlx::Error> {
        let links_raw: Option<String> = row.try_get("links")?;
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            links: parse_stored_links(links_raw.as_deref()),
            updated_at: row.try_get("updated_at")?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateProject {
    pub id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub links: Option<Vec<ProjectLink>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    /// An empty string clears the description.
    pub description: Option<String>,
    pub links: Option<Vec<ProjectLink>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssigneeKind {
    Human,
    Ai,
}

impl AssigneeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AssigneeKind::Human => "human",
            AssigneeKind::Ai => "ai",
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AssigneeMember {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub role_description: Option<String>,
    pub avatar_badge: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAssignee {
    pub id: Option<String>,
    pub name: String,
    pub kind: AssigneeKind,
    pub role_description: Option<String>,
    pub avatar_badge: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateAssignee {
    pub name: Option<String>,
    pub kind: Option<AssigneeKind>,
    /// An empty string clears role_description.
    pub role_description: Option<String>,
    /// An empty string clears avatar_badge.
    pub avatar_badge: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ColumnWithCards {
    #[serde(flatten)]
    pub column: Column,
    pub cards: Vec<Card>,
}

#[derive(Debug, Serialize)]
pub struct Board {
    pub title: String,
    pub columns: Vec<ColumnWithCards>,
    pub projects: Vec<Project>,
    pub assignees: Vec<AssigneeMember>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag {
    Build,
    Sell,
    Ops,
}

impl Tag {
    pub fn as_str(self) -> &'static str {
        match self {
            Tag::Build => "build",
            Tag::Sell => "sell",
            Tag::Ops => "ops",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Assignee {
    Human,
    Agent,
}

impl Assignee {
    pub fn as_str(self) -> &'static str {
        match self {
            Assignee::Human => "human",
            Assignee::Agent => "agent",
        }
    }
}

fn deserialize_opt_labels<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum LabelsInput {
        List(Vec<String>),
        Csv(String),
    }

    match Option::<LabelsInput>::deserialize(deserializer)? {
        None => Ok(None),
        Some(LabelsInput::List(list)) => Ok(Some(list)),
        Some(LabelsInput::Csv(s)) => Ok(Some(
            s.split(',').map(ToString::to_string).collect(),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateCard {
    /// Optional client-generated UUID so optimistic UI keeps a stable identity.
    pub id: Option<uuid::Uuid>,
    pub column_id: String,
    pub title: String,
    pub description: Option<String>,
    pub tag: Option<Tag>,
    pub assignee: Option<String>,
    pub status: Option<String>,
    pub priority: Option<i64>,
    pub pr_url: Option<String>,
    pub repo: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_labels")]
    pub labels: Option<Vec<String>>,
    /// Omit to append to the end of the column.
    pub position: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct PatchCard {
    pub column_id: Option<String>,
    pub position: Option<f64>,
    pub title: Option<String>,
    /// An empty string clears the description.
    pub description: Option<String>,
    pub tag: Option<Tag>,
    pub assignee: Option<String>,
    pub status: Option<String>,
    pub priority: Option<i64>,
    /// An empty string clears pr_url.
    pub pr_url: Option<String>,
    /// An empty string clears repo.
    pub repo: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_labels")]
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentVerb {
    /// Move into the in-progress column matching the card's tag and assign to the agent.
    Start,
    /// Move to Done.
    Complete,
    /// Only append a note.
    Note,
}

#[derive(Debug, Deserialize)]
pub struct AgentAction {
    /// Full UUID or a unique prefix (>= 4 chars, e.g. the 8-char short id shown in the UI).
    pub card_id: String,
    pub action: AgentVerb,
    pub note: Option<String>,
}
