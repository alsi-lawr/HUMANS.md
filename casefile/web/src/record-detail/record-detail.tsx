import { type ReactNode } from "react";
import {
  type Board,
  type Draft,
  type Identity,
  type Preview,
  type Record,
  type Relationship,
  sameIdentity,
  scopeLabel,
} from "../model";
import { Badge, classificationTone, identityKey, kindTone } from "../ui/badge";
import { ChangeControls, type MutationStatus } from "./change-review";
import { Editor } from "./editor";

export type DetailProps = Readonly<{
  record: Record | undefined;
  relationships: ReadonlyArray<Relationship>;
  boards: ReadonlyArray<Board>;
  draft: Draft | undefined;
  preview: Preview | undefined;
  capability: string;
  status: MutationStatus;
  message: string | undefined;
  onCapability: (value: string) => void;
  onDraft: (draft: Draft) => void;
  onPreview: () => void;
  onApply: () => void;
  onReconcile: () => void;
}>;
export const DetailPanel = ({
  record,
  relationships,
  boards,
  draft,
  preview,
  capability,
  status,
  message,
  onCapability,
  onDraft,
  onPreview,
  onApply,
  onReconcile,
}: DetailProps): ReactNode => (
  <aside className="min-h-0 overflow-y-auto border-l border-slate-800 bg-slate-950/90">
    <div className="border-b border-slate-800 p-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-slate-500">
        Record detail
      </p>
      {record === undefined ? (
        <p className="mt-3 text-sm text-slate-500">
          Select a card or record to inspect its governed context.
        </p>
      ) : (
        <>
          <div className="mt-3 flex flex-wrap gap-2">
            <Badge tone={classificationTone[record.classification]}>{record.classification}</Badge>
            {record.kind === undefined ? undefined : (
              <Badge tone={kindTone(record.kind)}>{record.kind}</Badge>
            )}
          </div>
          <h2 className="mt-3 text-lg font-semibold text-slate-100">{record.title}</h2>
          <p className="mt-2 break-all font-mono text-xs text-slate-500">{record.path}</p>
        </>
      )}
    </div>
    {record === undefined ? undefined : (
      <div className="space-y-6 p-4">
        <RelationshipList identity={record.identity} relationships={relationships} />
        <RecordFacts record={record} boards={boards} />
        {draft === undefined ? <ReadOnlyNotice /> : <Editor draft={draft} onChange={onDraft} />}
        {draft === undefined ? (
          <RecordContent title="Content" content={record.content} />
        ) : undefined}
        {draft !== undefined && status === "conflict" ? (
          <RecordContent title="Current canonical content" content={record.content} />
        ) : undefined}
        {draft === undefined ? undefined : (
          <ChangeControls
            capability={capability}
            status={status}
            message={message}
            onCapability={onCapability}
            onPreview={onPreview}
            onApply={onApply}
            onReconcile={onReconcile}
            preview={preview}
          />
        )}
      </div>
    )}
  </aside>
);

const RelationshipList = ({
  identity,
  relationships,
}: Readonly<{
  identity: Identity | undefined;
  relationships: ReadonlyArray<Relationship>;
}>): ReactNode => (
  <section>
    <h3 className="text-xs font-semibold uppercase tracking-widest text-slate-500">
      Relationships
    </h3>
    {relationships.length === 0 ? (
      <p className="mt-2 text-sm text-slate-500">No linked governed records.</p>
    ) : (
      <ul className="mt-3 space-y-2">
        {relationships.map((relationship) => {
          const linked =
            identity !== undefined && sameIdentity(relationship.source, identity)
              ? relationship.target
              : relationship.source;
          return (
            <li
              key={`${identityKey(relationship.source)}>${identityKey(relationship.target)}:${relationship.kind}`}
              className="rounded-lg border border-slate-800 bg-slate-900/50 px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <Badge tone="slate">{relationship.kind}</Badge>
                <span className="font-mono text-xs text-slate-500">{linked.identity}</span>
              </div>
              <p className="mt-2 text-xs text-slate-400">{scopeLabel(linked.scope)}</p>
            </li>
          );
        })}
      </ul>
    )}
  </section>
);
const RecordFacts = ({
  record,
  boards,
}: Readonly<{ record: Record; boards: ReadonlyArray<Board> }>): ReactNode => (
  <section>
    <h3 className="text-xs font-semibold uppercase tracking-widest text-slate-500">Context</h3>
    <dl className="mt-3 space-y-2 text-sm">
      {record.scope === undefined ? undefined : (
        <Fact label="Scope" value={scopeLabel(record.scope)} />
      )}
      {record.work_item === undefined ? undefined : (
        <>
          <Fact label="Status" value={record.work_item.status} />
          <Fact label="Rank" value={record.work_item.rank?.toString() ?? "Unranked"} />
          <Fact label="Confidence" value={record.work_item.confidence} />
        </>
      )}
      {record.kind === "board" ? (
        <Fact
          label="Columns"
          value={`${record.board?.columns.length ?? boards.length} configured`}
        />
      ) : undefined}
    </dl>
  </section>
);
const Fact = ({ label, value }: Readonly<{ label: string; value: string }>): ReactNode => (
  <div className="flex items-start justify-between gap-4">
    <dt className="text-slate-500">{label}</dt>
    <dd className="text-right text-slate-300">{value}</dd>
  </div>
);
const ReadOnlyNotice = (): ReactNode => (
  <section className="rounded-xl border border-slate-800 bg-slate-900/50 p-4">
    <h3 className="font-medium text-slate-200">Read-only record</h3>
    <p className="mt-2 text-sm leading-6 text-slate-400">
      Only complete governed tickets, epics, and boards can be replaced. This record remains visible
      without exposing its source as primary content.
    </p>
  </section>
);
const RecordContent = ({
  title,
  content,
}: Readonly<{ title: string; content: string | undefined }>): ReactNode =>
  content === undefined ? undefined : (
    <section>
      <h3 className="text-xs font-semibold uppercase tracking-widest text-slate-500">{title}</h3>
      <pre className="mt-3 max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-slate-800 bg-slate-900/50 p-3 font-sans text-sm leading-6 text-slate-300">
        {content}
      </pre>
    </section>
  );
