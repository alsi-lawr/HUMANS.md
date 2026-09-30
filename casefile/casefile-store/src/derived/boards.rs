use super::*;
use casefile_core::ProgressProjection;

struct Card<'a> {
    id: &'a str,
    kind: Kind,
    title: &'a str,
    rank: Option<u64>,
    order: usize,
}

type Buckets<'a> = BTreeMap<(bool, &'a str, &'static str), Vec<Card<'a>>>;

fn add_card<'a>(
    buckets: &mut Buckets<'a>,
    card: Card<'a>,
    disposition: &'a str,
    progress: Option<ProgressStatus>,
    include_disposition: bool,
) {
    let kind = if card.kind == Kind::Ticket {
        "ticket"
    } else {
        "epic"
    };
    if card.kind == Kind::Ticket && disposition == "accepted" {
        if let Some(progress) = progress {
            buckets
                .entry((true, progress.as_str(), kind))
                .or_default()
                .push(Card { ..card });
        }
    }
    if include_disposition {
        buckets
            .entry((false, disposition, kind))
            .or_default()
            .push(card);
    }
}

fn card_key<'a>(card: &'a Card<'_>) -> (u64, &'a str, usize) {
    (card.rank.unwrap_or(u64::MAX), card.id, card.order)
}

fn order_buckets(buckets: &mut Buckets<'_>) {
    for cards in buckets.values_mut() {
        cards.sort_by(|left, right| card_key(left).cmp(&card_key(right)));
    }
}

fn emit_board(board: &BoardDraft, scope: &RecordScope, buckets: &Buckets<'_>) -> DerivedBoard {
    let source = board.status_source == BoardStatusSource::Progress;
    let mut assigned = BTreeSet::new();
    let columns = board
        .columns
        .iter()
        .map(|column| {
            let mut streams = Vec::new();
            for status in &column.statuses {
                // The first matching column owns a status, including repeated statuses.
                if !assigned.insert(status.as_str())
                    || board
                        .filter_statuses
                        .as_ref()
                        .is_some_and(|filter| !filter.contains(status))
                {
                    continue;
                }
                for kind in ["ticket", "epic"] {
                    if board
                        .filter_kinds
                        .as_ref()
                        .is_some_and(|filter| !filter.iter().any(|value| value == kind))
                    {
                        continue;
                    }
                    if let Some(cards) = buckets.get(&(source, status.as_str(), kind)) {
                        streams.push((status.as_str(), cards.iter().peekable()));
                    }
                }
            }
            // Merge preordered buckets; neither rank sorting nor card ownership repeats per board.
            let mut cards = Vec::new();
            loop {
                let next = streams
                    .iter_mut()
                    .enumerate()
                    .filter_map(|(index, (_, stream))| {
                        stream.peek().map(|card| (index, card_key(card)))
                    })
                    .min_by(|left, right| left.1.cmp(&right.1))
                    .map(|(index, _)| index);
                let Some(next) = next else {
                    break;
                };
                let (status, stream) = &mut streams[next];
                let card = stream.next().expect("selected nonempty bucket");
                cards.push(DerivedCard {
                    identity: ScopedIdentity {
                        scope: scope.clone(),
                        identity: card.id.into(),
                    },
                    kind: card.kind,
                    title: card.title.into(),
                    status: (*status).into(),
                    rank: card.rank,
                });
            }
            DerivedBoardColumn {
                name: column.name.clone(),
                statuses: column.statuses.clone(),
                cards,
            }
        })
        .collect();
    DerivedBoard {
        identity: ScopedIdentity {
            scope: scope.clone(),
            identity: board.id.clone(),
        },
        title: board.title.clone(),
        status_source: board.status_source,
        filter_statuses: board.filter_statuses.clone(),
        filter_kinds: board.filter_kinds.clone(),
        columns,
    }
}

pub(super) fn derive_boards(records: &[DerivedRecord]) -> Vec<DerivedBoard> {
    let board_records: Vec<_> = records
        .iter()
        .filter(|record| {
            record.classification == Classification::Governed && record.board.is_some()
        })
        .collect();
    let mut needed: BTreeMap<&RecordScope, (bool, bool)> = BTreeMap::new();
    for record in &board_records {
        if let (Some(scope), Some(board)) = (&record.scope, &record.board) {
            let sources = needed.entry(scope).or_default();
            match board.status_source {
                BoardStatusSource::Disposition => sources.0 = true,
                BoardStatusSource::Progress => sources.1 = true,
            }
        }
    }
    let mut scopes: BTreeMap<&RecordScope, Buckets<'_>> = BTreeMap::new();
    for (order, record) in records.iter().enumerate() {
        let (Some(identity), Some(item), Some(kind)) =
            (&record.identity, &record.work_item, record.kind)
        else {
            continue;
        };
        let Some(&(disposition_needed, progress_needed)) = needed.get(&identity.scope) else {
            continue;
        };
        add_card(
            scopes.entry(&identity.scope).or_default(),
            Card {
                id: &identity.identity,
                kind,
                title: &item.title,
                rank: item.rank,
                order,
            },
            &item.status,
            record
                .progress
                .as_ref()
                .filter(|_| progress_needed)
                .map(|progress| progress.status),
            disposition_needed,
        );
    }
    for buckets in scopes.values_mut() {
        order_buckets(buckets);
    }
    let empty = Buckets::new();
    let mut boards: Vec<_> = board_records
        .into_iter()
        .filter_map(|record| {
            let (Some(board), Some(scope)) = (&record.board, &record.scope) else {
                return None;
            };
            Some(emit_board(
                board,
                scope,
                scopes.get(scope).unwrap_or(&empty),
            ))
        })
        .collect();
    boards.sort_by(|left, right| left.identity.cmp(&right.identity));
    boards
}

pub(crate) fn scoped_boards(
    entries: &[EntrySnapshot],
    boards: &[BoardDraft],
    progress: Option<&ProgressProjection>,
    project: &str,
    investigation: &str,
) -> Vec<DerivedBoard> {
    if boards.is_empty() {
        return Vec::new();
    }
    let scope = RecordScope {
        project: project.into(),
        investigation: Some(investigation.into()),
    };
    let disposition_needed = boards
        .iter()
        .any(|board| board.status_source == BoardStatusSource::Disposition);
    let progress_needed = boards
        .iter()
        .any(|board| board.status_source == BoardStatusSource::Progress);
    let mut buckets = Buckets::new();
    for (order, entry) in entries.iter().enumerate() {
        if entry.classification != Classification::Governed {
            continue;
        }
        let Some(RecordSummary::WorkItem {
            id,
            title,
            status,
            rank,
        }) = &entry.summary
        else {
            continue;
        };
        let Some(kind @ (Kind::Ticket | Kind::Epic)) = entry.kind else {
            continue;
        };
        let status_progress = progress
            .and_then(|progress| progress.tickets.get(id))
            .map_or(ProgressStatus::Unknown, |summary| summary.status);
        add_card(
            &mut buckets,
            Card {
                id,
                kind,
                title,
                rank: *rank,
                order,
            },
            status,
            progress_needed.then_some(status_progress),
            disposition_needed,
        );
    }
    order_buckets(&mut buckets);
    let mut result: Vec<_> = boards
        .iter()
        .map(|board| emit_board(board, &scope, &buckets))
        .collect();
    result.sort_by(|left, right| left.identity.cmp(&right.identity));
    result
}
