use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Column {
    pub id: String,
    pub title: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Card {
    pub id: String,
    pub column_id: String,
    pub title: String,
    pub description: Option<String>,
    pub tag: String,
    pub assignee: String,
    pub position: f64,
    pub updated_at: String,
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

#[derive(Debug, Deserialize)]
pub struct CreateCard {
    /// Optional client-generated UUID so optimistic UI keeps a stable identity.
    pub id: Option<uuid::Uuid>,
    pub column_id: String,
    pub title: String,
    pub description: Option<String>,
    pub tag: Option<Tag>,
    pub assignee: Option<Assignee>,
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
    pub assignee: Option<Assignee>,
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
