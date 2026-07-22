import {
  type EffectiveWriterBinding,
  type Record,
  type StrategyBindingState,
  type StrategyProjection,
  type StrategyWorker,
} from "../model";

export type StrategyPhase = "investigation" | "review" | "implementation";
export type PhaseIdentity =
  Readonly<{ tag: "known"; phase: StrategyPhase }> | Readonly<{ tag: "unknown"; label: string }>;
export type RootNode = Readonly<{
  id: "root";
  kind: "root";
  binding: "root";
}>;
export type WorkerNodeId = `worker:${string}:${number}`;
export type WorkerNode = Readonly<{
  id: WorkerNodeId;
  kind: "worker";
  worker: StrategyWorker;
  effective: EffectiveWriterBinding | undefined;
}>;
export type StrategyNode = RootNode | WorkerNode;
export type StrategyNodeId = StrategyNode["id"];
export type StrategyEdge = Readonly<{ source: "root"; target: WorkerNodeId }>;
export type StrategyGraph = Readonly<{
  phase: PhaseIdentity;
  root: RootNode;
  workers: ReadonlyArray<WorkerNode>;
  edges: ReadonlyArray<StrategyEdge>;
  projection: StrategyProjection;
}>;
export type StrategyGraphState =
  | Readonly<{ tag: "invalid" }>
  | Readonly<{ tag: "legacy" }>
  | Readonly<{ tag: "graph"; graph: StrategyGraph }>;

export const projectStrategyGraph = (record: Record): StrategyGraphState => {
  if (record.classification === "invalid") return { tag: "invalid" };
  if (record.strategy === undefined) return { tag: "legacy" };

  const projection = record.strategy;
  const phase = phaseIdentity(record.path);
  const implementationWriterCount = projection.workers.filter(
    (worker) => worker.role === "implementation-writer",
  ).length;
  const roleOccurrences = new Map<string, number>();
  const workers = projection.workers.map((worker): WorkerNode => {
    const occurrence = (roleOccurrences.get(worker.role) ?? 0) + 1;
    roleOccurrences.set(worker.role, occurrence);
    const id: WorkerNodeId = `worker:${worker.role}:${occurrence}`;
    return {
      id,
      kind: "worker",
      worker,
      effective:
        phase.tag === "known" &&
        phase.phase === "implementation" &&
        worker.role === "implementation-writer" &&
        implementationWriterCount === 1
          ? successfulBinding(projection.binding)
          : undefined,
    };
  });
  return {
    tag: "graph",
    graph: {
      phase,
      root: { id: "root", kind: "root", binding: projection.root_binding },
      workers,
      edges: workers.map((worker) => ({ source: "root", target: worker.id })),
      projection,
    },
  };
};

export const phaseIdentity = (path: string): PhaseIdentity => {
  const name = path.split("/").at(-1) ?? path;
  const label = name.endsWith(".toml") ? name.slice(0, -5) : name;
  switch (label) {
    case "investigation":
    case "review":
    case "implementation":
      return { tag: "known", phase: label };
    default:
      return { tag: "unknown", label };
  }
};

const successfulBinding = (
  binding: StrategyBindingState | null,
): EffectiveWriterBinding | undefined => {
  if (binding === null) return undefined;
  switch (binding.state) {
    case "absent":
    case "resolved":
      return binding.effective;
    case "pending":
    case "unresolved":
    case "invalid":
      return undefined;
  }
};
