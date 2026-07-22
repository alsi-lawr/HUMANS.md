use serde::{Deserialize, Serialize};

use crate::{board::BoardDraft, work_item::WorkItemDraft};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    Governed,
    Ungoverned,
    Invalid,
    Raw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Activation,
    ProjectMap,
    Request,
    Decision,
    Evidence,
    Review,
    Plan,
    Closeout,
    Strategy,
    StrategyBinding,
    Ticket,
    Epic,
    Board,
}

impl Kind {
    #[doc(hidden)]
    pub const fn is_writable(self) -> bool {
        matches!(self, Self::Ticket | Self::Epic | Self::Board)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecordSummary {
    Activation {
        projects: Vec<String>,
    },
    ProjectMap {
        projects: Vec<String>,
    },
    Markdown {
        title: String,
    },
    Strategy {
        strategy_id: String,
        phase: String,
        adapter: String,
    },
    StrategyBinding {
        binding: crate::strategy::StrategyBinding,
    },
    WorkItem {
        id: String,
        title: String,
        status: String,
        rank: Option<u64>,
    },
    Board {
        id: String,
        title: String,
        columns: Vec<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordDraft {
    Ticket(WorkItemDraft),
    Epic(WorkItemDraft),
    Board(BoardDraft),
}

impl RecordDraft {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Ticket(_) => Kind::Ticket,
            Self::Epic(_) => Kind::Epic,
            Self::Board(_) => Kind::Board,
        }
    }

    pub fn identity(&self) -> &str {
        match self {
            Self::Ticket(draft) | Self::Epic(draft) => &draft.id,
            Self::Board(draft) => &draft.id,
        }
    }
}
