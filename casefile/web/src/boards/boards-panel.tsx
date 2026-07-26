import { type ReactNode } from "react";
import { type BoardsState } from "./use-boards";
import { type Board, type Card, type Diagnostic, type Record, sameIdentity } from "../model";

export const BoardsPanel = ({
  state,
  records,
  onSelect,
}: Readonly<{
  state: BoardsState;
  records: ReadonlyArray<Record>;
  onSelect: (record: Record) => void;
}>): ReactNode => {
  if (state.tag === "no_scope")
    return <Empty message="Select an investigation to inspect its boards." />;
  if (state.tag === "loading") return <Empty message="Loading canonical board projection…" />;
  if (state.tag === "failure") return <Empty message={`Boards query failed: ${state.message}`} />;
  if (state.tag === "stale")
    return (
      <Empty message="Board projection is stale. Refresh to load the current investigation." />
    );
  if (state.tag === "invalid") return <InvalidBoardDefinitions diagnostics={state.diagnostics} />;
  if (state.boards.length === 0)
    return <Empty message="This investigation has no board definitions." />;
  return (
    <section>
      <header className="mb-4">
        <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">Boards</p>
        <h1 className="mt-1 text-xl font-semibold text-slate-100">Delivery boards</h1>
        <p className="mt-2 text-sm text-slate-500">
          Canonical placement is read-only in this workbench.
        </p>
      </header>
      <div className="space-y-8">
        {state.boards.map((board) => (
          <BoardView
            key={board.identity.identity}
            board={board}
            records={records}
            onSelect={onSelect}
          />
        ))}
      </div>
    </section>
  );
};

const BoardView = ({
  board,
  records,
  onSelect,
}: Readonly<{
  board: Board;
  records: ReadonlyArray<Record>;
  onSelect: (record: Record) => void;
}>): ReactNode => (
  <section aria-labelledby={`board-${board.identity.identity}`}>
    <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
      <h2 id={`board-${board.identity.identity}`} className="text-lg font-semibold text-slate-100">
        {board.title}
      </h2>
      <p className="text-xs font-medium uppercase tracking-widest text-slate-500">
        {board.status_source} status
      </p>
    </div>
    <div className="grid gap-3 overflow-x-auto pb-2 sm:grid-flow-col sm:grid-cols-none sm:auto-cols-[minmax(15rem,1fr)]">
      {board.columns.map((column) => (
        <section
          key={column.name}
          aria-label={`${board.title}: ${column.name}`}
          className="min-w-0 rounded-xl border border-slate-800 bg-slate-900/30 p-3"
        >
          <h3 className="text-sm font-semibold text-slate-200">
            {column.name} <span className="text-slate-500">{column.cards.length}</span>
          </h3>
          <div className="mt-3 space-y-2">
            {column.cards.length === 0 ? (
              <p className="text-sm text-slate-500">No cards.</p>
            ) : undefined}
            {column.cards.map((card) => (
              <BoardCard
                key={card.identity.identity}
                card={card}
                records={records}
                onSelect={onSelect}
              />
            ))}
          </div>
        </section>
      ))}
    </div>
  </section>
);

const BoardCard = ({
  card,
  records,
  onSelect,
}: Readonly<{
  card: Card;
  records: ReadonlyArray<Record>;
  onSelect: (record: Record) => void;
}>): ReactNode => {
  const matches = records.filter(
    (record) => record.identity !== undefined && sameIdentity(record.identity, card.identity),
  );
  if (matches.length !== 1) {
    const reason = matches.length === 0 ? "missing" : "ambiguous";
    return (
      <article className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-100">
        <p className="font-medium">{card.title}</p>
        <p className="mt-1 text-xs">
          Ticket detail is unavailable because its current identity is {reason}.
        </p>
      </article>
    );
  }
  const record = matches[0];
  if (record === undefined) return undefined;
  return (
    <button
      type="button"
      onClick={() => onSelect(record)}
      className="w-full rounded-lg border border-slate-700 bg-slate-950/70 p-3 text-left hover:border-blue-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-400"
    >
      <p className="text-sm font-medium text-slate-100">{card.title}</p>
      <p className="mt-1 text-xs text-slate-500">
        {card.identity.identity} · {card.status}
      </p>
    </button>
  );
};

const InvalidBoardDefinitions = ({
  diagnostics,
}: Readonly<{ diagnostics: ReadonlyArray<Diagnostic> }>): ReactNode => (
  <section className="rounded-xl border border-amber-500/30 bg-amber-500/10 p-6 text-sm text-amber-100">
    <h1 className="font-semibold">Board definitions or the progress log are invalid.</h1>
    <p className="mt-2 text-amber-100/80">
      Inspect Files or Diagnostics for the canonical validation details.
    </p>
    <ul className="mt-4 space-y-2">
      {diagnostics.map((diagnostic) => (
        <li key={`${diagnostic.path}:${diagnostic.code}:${diagnostic.field ?? ""}`}>
          <span className="font-mono text-xs font-semibold">{diagnostic.code}</span>
          <span className="ml-2">{diagnostic.message}</span>
        </li>
      ))}
    </ul>
  </section>
);

const Empty = ({ message }: Readonly<{ message: string }>): ReactNode => (
  <p className="rounded-xl border border-dashed border-slate-800 p-8 text-sm text-slate-500">
    {message}
  </p>
);
