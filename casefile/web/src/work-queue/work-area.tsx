import { type ReactNode } from "react";
import { type Board, type Identity, type Record, sameIdentity } from "../model";
import { Badge, classificationTone, identityKey, kindTone } from "../ui/badge";

export type BoardPanelProps = Readonly<{
  boards: ReadonlyArray<Board>;
  records: ReadonlyArray<Record>;
  selected: Identity | undefined;
  onSelect: (identity: Identity) => void;
}>;
export const BoardPanel = ({ boards, records, selected, onSelect }: BoardPanelProps): ReactNode => (
  <main className="min-w-0 overflow-y-auto bg-slate-950 p-4 lg:p-6">
    {boards.map((board) => (
      <BoardView
        key={identityKey(board.identity)}
        board={board}
        selected={selected}
        onSelect={onSelect}
      />
    ))}
    <WorkQueue records={records} selected={selected} onSelect={onSelect} />
  </main>
);

const BoardView = ({
  board,
  selected,
  onSelect,
}: Readonly<{
  board: Board;
  selected: Identity | undefined;
  onSelect: (identity: Identity) => void;
}>): ReactNode => (
  <section className="mb-8">
    <header className="mb-4 flex items-center justify-between gap-4">
      <div>
        <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">Board</p>
        <h1 className="mt-1 text-xl font-semibold text-slate-100">{board.title}</h1>
      </div>
      <Badge tone="emerald">{board.identity.identity}</Badge>
    </header>
    <div className="grid gap-4 xl:grid-cols-3">
      {board.columns.map((column) => (
        <BoardColumnView
          key={column.name}
          column={column}
          selected={selected}
          onSelect={onSelect}
        />
      ))}
    </div>
  </section>
);

const BoardColumnView = ({
  column,
  selected,
  onSelect,
}: Readonly<{
  column: Board["columns"][number];
  selected: Identity | undefined;
  onSelect: (identity: Identity) => void;
}>): ReactNode => (
  <section className="rounded-xl border border-slate-800 bg-slate-900/50 p-3">
    <header className="mb-3 flex items-center justify-between">
      <h2 className="text-sm font-semibold text-slate-200">{column.name}</h2>
      <span className="text-xs text-slate-500">{column.cards.length}</span>
    </header>
    <div className="space-y-2">
      {column.cards.length === 0 ? (
        <p className="rounded-lg border border-dashed border-slate-800 px-3 py-6 text-center text-xs text-slate-600">
          No matching records
        </p>
      ) : (
        column.cards.map((card) => (
          <button
            key={identityKey(card.identity)}
            type="button"
            onClick={() => onSelect(card.identity)}
            className={`w-full rounded-lg border p-3 text-left transition ${selected !== undefined && sameIdentity(selected, card.identity) ? "border-blue-500 bg-blue-500/10" : "border-slate-800 bg-slate-950 hover:border-slate-700"}`}
          >
            <div className="flex items-start justify-between gap-2">
              <Badge tone={kindTone(card.kind)}>{card.kind}</Badge>
              {card.rank === null ? undefined : (
                <span className="font-mono text-xs text-slate-500">#{card.rank}</span>
              )}
            </div>
            <p className="mt-3 line-clamp-2 text-sm font-medium text-slate-200">{card.title}</p>
            <p className="mt-2 text-xs text-slate-500">{card.status}</p>
          </button>
        ))
      )}
    </div>
  </section>
);

const WorkQueue = ({
  records,
  selected,
  onSelect,
}: Readonly<{
  records: ReadonlyArray<Record>;
  selected: Identity | undefined;
  onSelect: (identity: Identity) => void;
}>): ReactNode => (
  <section>
    <header className="mb-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">Work queue</p>
      <h1 className="mt-1 text-xl font-semibold text-slate-100">All records</h1>
    </header>
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {records.length === 0 ? (
        <p className="rounded-xl border border-dashed border-slate-800 p-8 text-sm text-slate-500">
          This scope has no records.
        </p>
      ) : (
        records.map((record) => (
          <RecordCard key={record.path} record={record} selected={selected} onSelect={onSelect} />
        ))
      )}
    </div>
  </section>
);

const RecordCard = ({
  record,
  selected,
  onSelect,
}: Readonly<{
  record: Record;
  selected: Identity | undefined;
  onSelect: (identity: Identity) => void;
}>): ReactNode => {
  const identity = record.identity;
  const active =
    identity !== undefined && selected !== undefined && sameIdentity(selected, identity);
  const content = (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={classificationTone[record.classification]}>{record.classification}</Badge>
        {record.kind === undefined ? undefined : (
          <Badge tone={kindTone(record.kind)}>{record.kind}</Badge>
        )}
      </div>
      <p className="mt-3 line-clamp-2 text-sm font-medium text-slate-200">{record.title}</p>
      <p className="mt-2 truncate font-mono text-xs text-slate-500">{record.path}</p>
    </>
  );
  if (identity === undefined)
    return (
      <article className="rounded-xl border border-slate-800 bg-slate-900/50 p-4">
        {content}
      </article>
    );
  return (
    <button
      type="button"
      onClick={() => onSelect(identity)}
      className={`rounded-xl border p-4 text-left ${active ? "border-blue-500 bg-blue-500/10" : "border-slate-800 bg-slate-900/50 hover:border-slate-700"}`}
    >
      {content}
    </button>
  );
};
