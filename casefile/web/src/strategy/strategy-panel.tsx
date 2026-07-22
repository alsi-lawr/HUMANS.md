import { type ReactNode } from "react";
import { type Diagnostic, type Record, type StrategyBindingState } from "../model";
import { Badge, classificationTone, kindTone } from "../ui/badge";
import { phaseIdentity, projectStrategyGraph } from "./graph-model";
import { StrategyGraph } from "./strategy-graph";

export type StrategyPanelProps = Readonly<{
  investigation: string | undefined;
  records: ReadonlyArray<Record>;
  selectedRecord: Record | undefined;
  selectedPath: string | undefined;
  diagnostics: ReadonlyArray<Diagnostic>;
  search: string;
  onSelect: (record: Record) => void;
}>;

export const StrategyPanel = ({
  investigation,
  records,
  selectedRecord,
  selectedPath,
  diagnostics,
  search,
  onSelect,
}: StrategyPanelProps): ReactNode => (
  <section>
    <header className="mb-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">Strategies</p>
      <h1 className="mt-1 text-xl font-semibold text-slate-100">Phase roles and constraints</h1>
      <p className="mt-2 text-sm leading-6 text-slate-500">
        Strategy records are durable and read-only. The graph reflects typed host data, not TOML
        interpreted by this browser.
      </p>
    </header>
    {investigation === undefined ? (
      <StateMessage message="Select an investigation before inspecting its strategies." />
    ) : records.length === 0 ? (
      <StateMessage
        message={
          search.trim() === ""
            ? "This investigation has no recognized strategy or binding records."
            : "No strategy records match the shared search filter."
        }
      />
    ) : (
      <>
        <div aria-label="Strategy records" className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {records.map((record) => (
            <StrategyCard
              key={record.path}
              record={record}
              selected={record.path === selectedPath}
              onSelect={onSelect}
            />
          ))}
        </div>
        <div className="mt-6">
          {selectedRecord === undefined ? (
            <StateMessage message="Select a phase strategy or writer binding to inspect it." />
          ) : (
            <StrategySelection
              key={selectedRecord.path}
              record={selectedRecord}
              diagnostics={diagnostics.filter(
                (diagnostic) => diagnostic.path === selectedRecord.path,
              )}
            />
          )}
        </div>
      </>
    )}
  </section>
);

const StrategyCard = ({
  record,
  selected,
  onSelect,
}: Readonly<{
  record: Record;
  selected: boolean;
  onSelect: (record: Record) => void;
}>): ReactNode => {
  const title =
    record.kind === "strategy_binding"
      ? "Implementation writer binding"
      : `${phaseLabel(record.path)} phase`;
  return (
    <button
      type="button"
      onClick={() => onSelect(record)}
      className={selected ? selectedCardClass : cardClass}
    >
      <span className="flex flex-wrap items-center gap-2">
        <Badge tone={classificationTone[record.classification]}>{record.classification}</Badge>
        {record.kind === undefined ? undefined : (
          <Badge tone={kindTone(record.kind)}>{record.kind.replace("_", " ")}</Badge>
        )}
      </span>
      <span className="mt-3 block text-sm font-semibold text-slate-100">{title}</span>
      <span className="mt-2 block truncate font-mono text-xs text-slate-500">
        {fileName(record.path)}
      </span>
    </button>
  );
};

const StrategySelection = ({
  record,
  diagnostics,
}: Readonly<{ record: Record; diagnostics: ReadonlyArray<Diagnostic> }>): ReactNode => {
  if (record.kind === "strategy_binding")
    return <BindingSummary record={record} diagnostics={diagnostics} />;
  const state = projectStrategyGraph(record);
  switch (state.tag) {
    case "invalid":
      return (
        <NonGraphState
          title="Invalid strategy record"
          message="The host recognized this phase record but rejected its contents. No graph is inferred."
          diagnostics={diagnostics}
        />
      );
    case "legacy":
      return (
        <NonGraphState
          title="Legacy strategy without a typed projection"
          message="Exact source remains available, but this historical strategy does not declare canonical graph data."
          diagnostics={diagnostics}
        />
      );
    case "graph":
      return <StrategyGraph graph={state.graph} />;
  }
};

const BindingSummary = ({
  record,
  diagnostics,
}: Readonly<{ record: Record; diagnostics: ReadonlyArray<Diagnostic> }>): ReactNode => {
  if (record.classification === "invalid" || record.strategy_binding === undefined)
    return (
      <NonGraphState
        title="Invalid writer binding"
        message="No successful effective runtime is available. Inspect Diagnostics and exact Source for the rejected record."
        diagnostics={diagnostics}
      />
    );
  const binding = record.strategy_binding;
  return (
    <section className="rounded-xl border border-slate-800 bg-slate-900/30 p-5">
      <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">
        Non-graph state
      </p>
      <h2 className="mt-1 text-lg font-semibold text-slate-100">Implementation writer binding</h2>
      <p className="mt-2 text-sm leading-6 text-slate-400">
        This record selects runtime configuration; it does not declare graph nodes or edges. Exact
        TOML remains in Source.
      </p>
      <dl className="mt-4 max-w-xl space-y-2 text-sm">
        <Fact label="Adapter" value={binding.adapter} />
        <Fact label="Role" value={binding.role} />
        <Fact label="Requested runtime" value={`${binding.model} / ${binding.reasoning_effort}`} />
        <Fact
          label="Resolution"
          value={`${binding.resolution.mode}: ${binding.resolution.value}`}
        />
        <Fact label="State" value={binding.state.state} />
        <SuccessfulBindingFacts state={binding.state} />
      </dl>
      <BindingStateMessage state={binding.state} />
      <DiagnosticList diagnostics={diagnostics} />
    </section>
  );
};

const SuccessfulBindingFacts = ({
  state,
}: Readonly<{ state: StrategyBindingState }>): ReactNode => {
  switch (state.state) {
    case "absent":
    case "resolved":
      return (
        <>
          <Fact
            label="Effective runtime"
            value={`${state.effective.model} / ${state.effective.reasoning_effort}`}
          />
          <Fact label="Effective source" value={state.effective.source} />
        </>
      );
    case "pending":
    case "unresolved":
    case "invalid":
      return undefined;
  }
};

const BindingStateMessage = ({ state }: Readonly<{ state: StrategyBindingState }>): ReactNode => {
  const message = bindingStateMessage(state);
  return message === undefined ? undefined : (
    <p className="mt-4 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm leading-6 text-amber-100">
      {message}
    </p>
  );
};

const bindingStateMessage = (state: StrategyBindingState): string | undefined => {
  switch (state.state) {
    case "absent":
      return "No binding file is selected; the historical matrix default is effective.";
    case "resolved":
      return undefined;
    case "pending":
      return "This binding is pending because no implementation strategy is selected. No effective runtime is shown.";
    case "unresolved":
      return "The selected runtime cannot be resolved. No effective runtime is shown.";
    case "invalid":
      return "The binding is invalid. No effective runtime is shown.";
  }
};

const NonGraphState = ({
  title,
  message,
  diagnostics,
}: Readonly<{
  title: string;
  message: string;
  diagnostics: ReadonlyArray<Diagnostic>;
}>): ReactNode => (
  <section className="rounded-xl border border-amber-500/30 bg-amber-500/10 p-5">
    <p className="text-xs font-semibold uppercase tracking-widest text-amber-300">
      Non-graph state
    </p>
    <h2 className="mt-2 text-lg font-semibold text-slate-100">{title}</h2>
    <p className="mt-2 text-sm leading-6 text-slate-300">{message}</p>
    <DiagnosticList diagnostics={diagnostics} />
  </section>
);

const DiagnosticList = ({
  diagnostics,
}: Readonly<{ diagnostics: ReadonlyArray<Diagnostic> }>): ReactNode => (
  <section className="mt-5" aria-labelledby="strategy-diagnostics-heading">
    <h3
      id="strategy-diagnostics-heading"
      className="text-xs font-semibold uppercase tracking-widest text-slate-500"
    >
      Diagnostics
    </h3>
    {diagnostics.length === 0 ? (
      <p className="mt-2 text-sm text-slate-500">No diagnostics for this record.</p>
    ) : (
      <ul className="mt-3 space-y-2">
        {diagnostics.map((diagnostic) => (
          <li
            key={`${diagnostic.path}:${diagnostic.code}:${diagnostic.field ?? ""}:${diagnostic.section ?? ""}`}
            className="rounded-lg border border-rose-500/30 bg-rose-500/10 p-3"
          >
            <p className="font-mono text-xs font-semibold text-rose-200">{diagnostic.code}</p>
            <p className="mt-1 text-sm text-slate-300">{diagnostic.message}</p>
          </li>
        ))}
      </ul>
    )}
  </section>
);

const Fact = ({ label, value }: Readonly<{ label: string; value: string }>): ReactNode => (
  <div className="flex items-start justify-between gap-4">
    <dt className="text-slate-500">{label}</dt>
    <dd className="text-right text-slate-300">{value}</dd>
  </div>
);
const StateMessage = ({ message }: Readonly<{ message: string }>): ReactNode => (
  <p className="rounded-xl border border-dashed border-slate-800 p-8 text-sm text-slate-500">
    {message}
  </p>
);
const phaseLabel = (path: string): string => {
  const phase = phaseIdentity(path);
  return phase.tag === "known" ? phase.phase : phase.label;
};
const fileName = (path: string): string => path.split("/").at(-1) ?? path;
const cardClass =
  "rounded-xl border border-slate-800 bg-slate-900/50 p-4 text-left hover:border-slate-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-400";
const selectedCardClass =
  "rounded-xl border border-blue-500 bg-blue-500/10 p-4 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-300";
