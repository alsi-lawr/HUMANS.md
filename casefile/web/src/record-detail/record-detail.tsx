import { type ReactNode, useState } from "react";
import {
  type Draft,
  type Diagnostic,
  type Identity,
  type Preview,
  type Record,
  type Relationship,
  scopeLabel,
} from "../model";
import { Badge, classificationTone, identityKey, kindTone } from "../ui/badge";
import { ChangeControls, type MutationStatus } from "./change-review";
import { Editor } from "./editor";

export type DetailProps = Readonly<{
  record: Record | undefined;
  diagnostics: ReadonlyArray<Diagnostic>;
  relationships: ReadonlyArray<Relationship>;
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
  diagnostics,
  relationships,
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
  <aside className="min-h-0 border-l border-slate-800 bg-slate-950/90 lg:overflow-y-auto">
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
        <RecordTabs
          record={record}
          diagnostics={diagnostics}
          relationships={relationships}
          draft={draft}
          preview={preview}
          capability={capability}
          status={status}
          message={message}
          onCapability={onCapability}
          onDraft={onDraft}
          onPreview={onPreview}
          onApply={onApply}
          onReconcile={onReconcile}
        />
      </div>
    )}
  </aside>
);

type DetailTab = "overview" | "rendered" | "source" | "diagnostics";

const RecordTabs = ({
  record,
  diagnostics,
  relationships,
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
}: Omit<DetailProps, "record"> & Readonly<{ record: Record }>): ReactNode => {
  const [tab, setTab] = useState<DetailTab>("overview");
  return (
    <>
      <nav
        aria-label="Record detail tabs"
        className="flex flex-wrap gap-1 border-b border-slate-800 pb-3"
      >
        {detailTabs.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            className={tab === id ? activeTabClass : inactiveTabClass}
          >
            {label}
          </button>
        ))}
      </nav>
      {tab === "overview" ? (
        <>
          <RelationshipList identity={record.identity} relationships={relationships} />
          <RecordFacts record={record} />
          {draft === undefined ? <ReadOnlyNotice /> : <Editor draft={draft} onChange={onDraft} />}
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
        </>
      ) : undefined}
      {tab === "rendered" ? <RenderedContent content={record.rendered_markdown} /> : undefined}
      {tab === "source" ? (
        <RecordContent title="Exact source" content={record.content} />
      ) : undefined}
      {tab === "diagnostics" ? (
        <RecordDiagnostics
          diagnostics={diagnostics.filter((diagnostic) => diagnostic.path === record.path)}
        />
      ) : undefined}
    </>
  );
};

const detailTabs: ReadonlyArray<Readonly<{ id: DetailTab; label: string }>> = [
  { id: "overview", label: "Overview" },
  { id: "rendered", label: "Rendered" },
  { id: "source", label: "Source" },
  { id: "diagnostics", label: "Diagnostics" },
];

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
            identity !== undefined &&
            relationship.source.identity === identity.identity &&
            relationship.source.scope.project === identity.scope.project &&
            relationship.source.scope.investigation === identity.scope.investigation
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
const RecordFacts = ({ record }: Readonly<{ record: Record }>): ReactNode => (
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
        <Fact label="Columns" value={`${record.board?.columns.length ?? 0} configured`} />
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

const RenderedContent = ({ content }: Readonly<{ content: string | undefined }>): ReactNode =>
  content === undefined ? (
    <p className="text-sm text-slate-500">This record does not have rendered Markdown.</p>
  ) : (
    <section>
      <h3 className="text-xs font-semibold uppercase tracking-widest text-slate-500">
        Rendered Markdown
      </h3>
      <article
        className="mt-3 max-h-96 overflow-auto rounded-lg border border-slate-800 bg-slate-900/50 p-3 text-sm leading-6 text-slate-300 [&_a]:text-blue-300 [&_code]:rounded [&_code]:bg-slate-800 [&_code]:px-1 [&_pre]:overflow-auto [&_table]:w-full [&_td]:border [&_td]:border-slate-700 [&_td]:p-2 [&_th]:border [&_th]:border-slate-700 [&_th]:p-2"
        dangerouslySetInnerHTML={{ __html: content }}
      />
    </section>
  );

const RecordDiagnostics = ({
  diagnostics,
}: Readonly<{ diagnostics: ReadonlyArray<Diagnostic> }>): ReactNode => (
  <section>
    <h3 className="text-xs font-semibold uppercase tracking-widest text-slate-500">
      Record diagnostics
    </h3>
    {diagnostics.length === 0 ? (
      <p className="mt-3 text-sm text-emerald-300">No diagnostics for this record.</p>
    ) : (
      <ul className="mt-3 space-y-3">
        {diagnostics.map((diagnostic) => (
          <li
            key={`${diagnostic.path}:${diagnostic.code}:${diagnostic.field ?? ""}:${diagnostic.section ?? ""}`}
            className="rounded-lg border border-rose-500/30 bg-rose-500/10 p-3"
          >
            <p className="font-mono text-xs font-semibold text-rose-200">{diagnostic.code}</p>
            <p className="mt-2 text-sm leading-6 text-slate-300">{diagnostic.message}</p>
            {diagnostic.field === undefined ? undefined : (
              <p className="mt-2 text-xs text-slate-500">Field: {diagnostic.field}</p>
            )}
            {diagnostic.section === undefined ? undefined : (
              <p className="mt-1 text-xs text-slate-500">Section: {diagnostic.section}</p>
            )}
          </li>
        ))}
      </ul>
    )}
  </section>
);

const activeTabClass = "rounded-lg bg-blue-500/15 px-3 py-2 text-sm font-medium text-blue-200";
const inactiveTabClass =
  "rounded-lg px-3 py-2 text-sm font-medium text-slate-400 hover:bg-slate-900 hover:text-slate-200";
