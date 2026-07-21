import { type ReactNode } from "react";
import { type BoardColumn, type BoardDraft } from "../model";

const fieldClass =
  "mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none placeholder:text-slate-600 focus:border-blue-400";

export const BoardEditor = ({
  board,
  onChange,
}: Readonly<{ board: BoardDraft; onChange: (board: BoardDraft) => void }>): ReactNode => (
  <section>
    <h3 className="text-xs font-semibold uppercase tracking-widest text-slate-500">Edit board</h3>
    <p className="mt-2 text-xs text-slate-500">{board.id}</p>
    <div className="mt-4 space-y-3">
      <label className="block text-xs font-medium text-slate-400">
        Title
        <input
          className={fieldClass}
          value={board.title}
          onChange={(event) => onChange({ ...board, title: event.target.value })}
        />
      </label>
      <BoardFilter
        label="Filter statuses"
        values={board.filter_statuses}
        onChange={(values) => onChange({ ...board, filter_statuses: values })}
      />
      <BoardFilter
        label="Filter kinds"
        values={board.filter_kinds}
        onChange={(values) => onChange({ ...board, filter_kinds: values })}
      />
      {board.columns.map((column, index) => (
        <BoardColumnEditor
          key={column.editor_key}
          column={column}
          onChange={(value) =>
            onChange({
              ...board,
              columns: board.columns.map((entry, current) => (current === index ? value : entry)),
            })
          }
          onRemove={() =>
            onChange({ ...board, columns: board.columns.filter((_, current) => current !== index) })
          }
        />
      ))}
      <button
        type="button"
        className="rounded-lg border border-dashed border-slate-700 px-3 py-2 text-sm text-slate-300 hover:border-blue-400 hover:text-blue-200"
        onClick={() =>
          onChange({
            ...board,
            columns: [
              ...board.columns,
              { editor_key: crypto.randomUUID(), name: "New column", statuses: [] },
            ],
          })
        }
      >
        Add column
      </button>
    </div>
  </section>
);
const BoardFilter = ({
  label,
  values,
  onChange,
}: Readonly<{
  label: string;
  values: ReadonlyArray<string> | null | undefined;
  onChange: (values: ReadonlyArray<string> | undefined) => void;
}>): ReactNode => (
  <label className="block text-xs font-medium text-slate-400">
    {label}
    <input
      className={fieldClass}
      value={values?.join(", ") ?? ""}
      onChange={(event) => {
        const text = event.target.value.trim();
        onChange(
          text === ""
            ? undefined
            : text
                .split(",")
                .map((entry) => entry.trim())
                .filter((entry) => entry.length > 0),
        );
      }}
    />
  </label>
);
const BoardColumnEditor = ({
  column,
  onChange,
  onRemove,
}: Readonly<{
  column: BoardColumn;
  onChange: (column: BoardColumn) => void;
  onRemove: () => void;
}>): ReactNode => (
  <fieldset className="rounded-lg border border-slate-800 bg-slate-900/50 p-3">
    <legend className="px-1 text-xs text-slate-500">Column</legend>
    <label className="block text-xs font-medium text-slate-400">
      Name
      <input
        className={fieldClass}
        value={column.name}
        onChange={(event) => onChange({ ...column, name: event.target.value })}
      />
    </label>
    <label className="mt-3 block text-xs font-medium text-slate-400">
      Statuses
      <input
        className={fieldClass}
        value={column.statuses.join(", ")}
        onChange={(event) =>
          onChange({
            ...column,
            statuses: event.target.value
              .split(",")
              .map((entry) => entry.trim())
              .filter((entry) => entry.length > 0),
          })
        }
      />
    </label>
    <button
      type="button"
      className="mt-3 text-xs font-medium text-rose-300 hover:text-rose-200"
      onClick={onRemove}
    >
      Remove column
    </button>
  </fieldset>
);
