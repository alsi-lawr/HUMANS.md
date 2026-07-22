import { type ReactNode, useState } from "react";
import { type StrategyBindingState } from "../model";
import { type StrategyGraph as Graph, type StrategyNode, type StrategyNodeId } from "./graph-model";

export type StrategyGraphProps = Readonly<{ graph: Graph }>;

export const StrategyGraph = ({ graph }: StrategyGraphProps): ReactNode => {
  const [selectedNodeId, setSelectedNodeId] = useState<StrategyNodeId>(graph.root.id);
  const selectedNode = selectedNodeFor(graph, selectedNodeId);
  const phaseLabel = graph.phase.tag === "known" ? graph.phase.phase : graph.phase.label;

  return (
    <section
      aria-labelledby="strategy-graph-heading"
      className="rounded-xl border border-slate-800 bg-slate-900/30 p-4 sm:p-5"
    >
      <header>
        <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">
          {phaseLabel} phase
        </p>
        <h2 id="strategy-graph-heading" className="mt-1 text-lg font-semibold text-slate-100">
          Declared role graph
        </h2>
        <p className="mt-2 text-sm leading-6 text-slate-400">
          Connectors show declarations from the canonical root only. They do not imply
          worker-to-worker execution.
        </p>
      </header>
      <div className="mt-5 grid gap-5 md:grid-cols-3">
        <div className="md:col-span-2">
          <div className="flex justify-center">
            <NodeButton
              node={graph.root}
              selected={selectedNode.id === graph.root.id}
              onSelect={setSelectedNodeId}
            />
          </div>
          {graph.workers.length === 0 ? (
            <p className="mt-5 rounded-lg border border-dashed border-slate-700 p-4 text-center text-sm text-slate-400">
              No workers declared. This is a valid root-only strategy.
            </p>
          ) : (
            <ul className="mt-1 grid gap-3 sm:grid-cols-2">
              {graph.workers.map((worker) => (
                <li key={worker.id} className="flex flex-col items-center">
                  <div
                    aria-hidden="true"
                    className="flex w-full max-w-xs items-center gap-2 py-2 text-xs text-blue-300"
                  >
                    <span>root</span>
                    <span className="flex-1 border-t border-blue-400/50" />
                  </div>
                  <NodeButton
                    node={worker}
                    selected={selectedNode.id === worker.id}
                    onSelect={setSelectedNodeId}
                  />
                </li>
              ))}
            </ul>
          )}
        </div>
        <NodeDetail graph={graph} node={selectedNode} />
      </div>
      <StrategyConstraints graph={graph} />
    </section>
  );
};

const NodeButton = ({
  node,
  selected,
  onSelect,
}: Readonly<{
  node: StrategyNode;
  selected: boolean;
  onSelect: (nodeId: StrategyNodeId) => void;
}>): ReactNode => {
  const label = node.kind === "root" ? "Root orchestrator" : node.worker.role;
  const description = node.kind === "root" ? "canonical root" : node.worker.platform_profile;
  return (
    <button
      type="button"
      aria-label={`Inspect ${label}`}
      aria-pressed={selected}
      onClick={() => onSelect(node.id)}
      className={selected ? selectedNodeClass : nodeClass}
    >
      <span className="block text-sm font-semibold text-slate-100">{label}</span>
      <span className="mt-1 block text-xs text-slate-400">{description}</span>
    </button>
  );
};

const NodeDetail = ({ graph, node }: Readonly<{ graph: Graph; node: StrategyNode }>): ReactNode => (
  <section
    aria-live="polite"
    aria-labelledby="strategy-node-detail-heading"
    className="rounded-xl border border-slate-800 bg-slate-950/70 p-4"
  >
    <p className="text-xs font-semibold uppercase tracking-widest text-slate-500">Selected node</p>
    <h3 id="strategy-node-detail-heading" className="mt-2 font-semibold text-slate-100">
      {node.kind === "root" ? "Root orchestrator" : node.worker.role}
    </h3>
    <dl className="mt-4 space-y-2 text-sm">
      {node.kind === "root" ? (
        <>
          <Fact label="Binding" value={node.binding} />
          <Fact
            label="Capacity"
            value={`${graph.projection.limits.max_concurrent_subagents} subagents`}
          />
          <Fact label="Depth" value={graph.projection.limits.max_depth.toString()} />
        </>
      ) : (
        <WorkerFacts graph={graph} node={node} />
      )}
    </dl>
  </section>
);

const WorkerFacts = ({
  graph,
  node,
}: Readonly<{
  graph: Graph;
  node: Extract<StrategyNode, Readonly<{ kind: "worker" }>>;
}>): ReactNode => {
  const declaredRuntime =
    node.worker.runtime.tag === "unspecified"
      ? "Not declared"
      : `${node.worker.runtime.model} / ${node.worker.runtime.reasoning_effort}`;
  const writerCount = graph.workers.filter(
    (worker) => worker.worker.role === "implementation-writer",
  ).length;
  const overlayState = implementationOverlayState(graph, node.worker.role, writerCount);
  return (
    <>
      <Fact label="Count" value={`${node.worker.minimum_count}–${node.worker.maximum_count}`} />
      <Fact label="May spawn" value={yesNo(node.worker.can_spawn_subagents)} />
      <Fact label="Profile" value={node.worker.platform_profile} />
      <Fact label="Declared runtime" value={declaredRuntime} />
      {node.effective === undefined ? undefined : (
        <>
          <Fact
            label="Effective runtime"
            value={`${node.effective.model} / ${node.effective.reasoning_effort}`}
          />
          <Fact label="Effective source" value={node.effective.source} />
        </>
      )}
      {overlayState === undefined ? undefined : <Fact label="Binding" value={overlayState} />}
    </>
  );
};

const implementationOverlayState = (
  graph: Graph,
  role: string,
  writerCount: number,
): string | undefined => {
  if (graph.phase.tag !== "known" || graph.phase.phase !== "implementation") return undefined;
  if (role !== "implementation-writer") return undefined;
  if (writerCount !== 1)
    return "Effective binding is not overlaid because the phase does not declare exactly one implementation writer.";
  return unsuccessfulBindingMessage(graph.projection.binding);
};

const unsuccessfulBindingMessage = (binding: StrategyBindingState | null): string | undefined => {
  if (binding === null) return "No effective binding state was reported for this phase.";
  switch (binding.state) {
    case "absent":
    case "resolved":
      return undefined;
    case "pending":
      return "The implementation writer binding is pending a selected implementation strategy.";
    case "unresolved":
      return "The implementation writer binding is unresolved; no effective runtime is shown.";
    case "invalid":
      return "The implementation writer binding is invalid; no effective runtime is shown.";
  }
};

const StrategyConstraints = ({ graph }: Readonly<{ graph: Graph }>): ReactNode => {
  const { coordination, limits, requirements } = graph.projection;
  return (
    <section
      aria-labelledby="strategy-constraints-heading"
      className="mt-6 border-t border-slate-800 pt-5"
    >
      <h3 id="strategy-constraints-heading" className="text-sm font-semibold text-slate-200">
        Constraints and coordination
      </h3>
      <div className="mt-4 grid gap-4 sm:grid-cols-2">
        <ConstraintGroup title="Limits and requirements">
          <Fact label="Concurrent subagents" value={limits.max_concurrent_subagents.toString()} />
          <Fact label="Maximum depth" value={limits.max_depth.toString()} />
          <Fact
            label="Capabilities"
            value={
              requirements.capabilities.length === 0
                ? "None declared"
                : requirements.capabilities.join(", ")
            }
          />
        </ConstraintGroup>
        <ConstraintGroup title="Coordination">
          <Fact
            label="Batch at capacity"
            value={yesNo(coordination.batch_when_capacity_exceeded)}
          />
          <Fact
            label="Review before ticket"
            value={yesNo(coordination.candidate_review_before_ticket)}
          />
          <Fact
            label="Shared ticket storage"
            value={yesNo(coordination.shared_ticket_storage_required)}
          />
        </ConstraintGroup>
      </div>
      {coordination.pipeline === undefined ? (
        <p className="mt-4 text-sm text-slate-500">No pipeline gates declared for this phase.</p>
      ) : (
        <ConstraintGroup title="Pipeline gates">
          <Fact
            label="Active tickets"
            value={coordination.pipeline.maximum_active_tickets.toString()}
          />
          <Fact
            label="Read-only look-ahead"
            value={yesNo(coordination.pipeline.look_ahead_read_only)}
          />
          <Fact
            label="Independent dependencies"
            value={yesNo(coordination.pipeline.require_dependency_independence)}
          />
          <Fact
            label="Disjoint write paths"
            value={yesNo(coordination.pipeline.require_disjoint_write_paths)}
          />
          <Fact
            label="Immutable review commits"
            value={yesNo(coordination.pipeline.immutable_review_commits)}
          />
          <Fact
            label="Corrections preempt"
            value={yesNo(coordination.pipeline.corrections_preempt_forward_work)}
          />
        </ConstraintGroup>
      )}
    </section>
  );
};

const ConstraintGroup = ({
  title,
  children,
}: Readonly<{ title: string; children: ReactNode }>): ReactNode => (
  <section className="rounded-lg border border-slate-800 bg-slate-950/50 p-4">
    <h4 className="text-xs font-semibold uppercase tracking-widest text-slate-500">{title}</h4>
    <dl className="mt-3 space-y-2 text-sm">{children}</dl>
  </section>
);

const Fact = ({ label, value }: Readonly<{ label: string; value: string }>): ReactNode => (
  <div className="flex items-start justify-between gap-4">
    <dt className="text-slate-500">{label}</dt>
    <dd className="text-right text-slate-300">{value}</dd>
  </div>
);

const selectedNodeFor = (graph: Graph, nodeId: StrategyNodeId): StrategyNode =>
  nodeId === graph.root.id
    ? graph.root
    : (graph.workers.find((worker) => worker.id === nodeId) ?? graph.root);
const yesNo = (value: boolean): string => (value ? "Yes" : "No");
const nodeClass =
  "w-full max-w-xs rounded-xl border border-slate-700 bg-slate-900 px-4 py-3 text-left hover:border-blue-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-400";
const selectedNodeClass =
  "w-full max-w-xs rounded-xl border border-blue-400 bg-blue-500/10 px-4 py-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-300";
