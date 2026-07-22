import { type ReactNode } from "react";
import { type WorkItemDraft } from "../model";

type WorkField =
  | "title"
  | "reported_by_role"
  | "reported_by_agent"
  | "source_commit"
  | "created_at"
  | "updated_at"
  | "confidence"
  | "requirement_and_evidence"
  | "impact"
  | "resolution_boundary"
  | "acceptance_criteria"
  | "verification"
  | "relationships_and_duplicate_analysis"
  | "review_and_disposition_history";
type WorkList = "decision_refs" | "related_tickets" | "supersedes" | "superseded_by";
type TextField = Readonly<{ field: WorkField; label: string; multiline: boolean }>;
const textFields: ReadonlyArray<TextField> = [
  { field: "title", label: "Title", multiline: false },
  { field: "reported_by_role", label: "Reported by role", multiline: false },
  { field: "reported_by_agent", label: "Reported by agent", multiline: false },
  { field: "source_commit", label: "Source commit", multiline: false },
  { field: "created_at", label: "Created at", multiline: false },
  { field: "updated_at", label: "Updated at", multiline: false },
  { field: "confidence", label: "Confidence", multiline: false },
  { field: "requirement_and_evidence", label: "Requirement and evidence", multiline: true },
  { field: "impact", label: "Impact", multiline: true },
  { field: "resolution_boundary", label: "Resolution boundary", multiline: true },
  { field: "acceptance_criteria", label: "Acceptance criteria", multiline: true },
  { field: "verification", label: "Verification", multiline: true },
  {
    field: "relationships_and_duplicate_analysis",
    label: "Relationships and duplicate analysis",
    multiline: true,
  },
  {
    field: "review_and_disposition_history",
    label: "Review and disposition history",
    multiline: true,
  },
];
const listFields: ReadonlyArray<WorkList> = [
  "decision_refs",
  "related_tickets",
  "supersedes",
  "superseded_by",
];
const updateText = (item: WorkItemDraft, field: WorkField, value: string): WorkItemDraft => {
  switch (field) {
    case "title":
      return { ...item, title: value };
    case "reported_by_role":
      return { ...item, reported_by_role: value };
    case "reported_by_agent":
      return { ...item, reported_by_agent: value };
    case "source_commit":
      return { ...item, source_commit: value };
    case "created_at":
      return { ...item, created_at: value };
    case "updated_at":
      return { ...item, updated_at: value };
    case "confidence":
      return { ...item, confidence: value };
    case "requirement_and_evidence":
      return { ...item, requirement_and_evidence: value };
    case "impact":
      return { ...item, impact: value };
    case "resolution_boundary":
      return { ...item, resolution_boundary: value };
    case "acceptance_criteria":
      return { ...item, acceptance_criteria: value };
    case "verification":
      return { ...item, verification: value };
    case "relationships_and_duplicate_analysis":
      return { ...item, relationships_and_duplicate_analysis: value };
    case "review_and_disposition_history":
      return { ...item, review_and_disposition_history: value };
  }
};
const updateList = (item: WorkItemDraft, field: WorkList, value: string): WorkItemDraft => {
  const parsed = value
    .split(",")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
  switch (field) {
    case "decision_refs":
      return { ...item, decision_refs: parsed };
    case "related_tickets":
      return { ...item, related_tickets: parsed };
    case "supersedes":
      return { ...item, supersedes: parsed };
    case "superseded_by":
      return { ...item, superseded_by: parsed };
  }
};

const fieldClass =
  "mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none placeholder:text-slate-600 focus:border-blue-400";
export const WorkItemEditor = ({
  kind,
  item,
  onChange,
}: Readonly<{
  kind: "ticket" | "epic";
  item: WorkItemDraft;
  onChange: (item: WorkItemDraft) => void;
}>): ReactNode => (
  <section>
    <h3 className="text-xs font-semibold uppercase tracking-widest text-slate-500">Edit {kind}</h3>
    <p className="mt-2 text-xs text-slate-500">
      {item.id} · {item.project} / {item.investigation}
    </p>
    <div className="mt-4 space-y-3">
      {textFields.map(({ field, label, multiline }) => (
        <label key={field} className="block text-xs font-medium text-slate-400">
          {label}
          {multiline ? (
            <textarea
              className={`${fieldClass} min-h-24`}
              value={item[field]}
              onChange={(event) => onChange(updateText(item, field, event.target.value))}
            />
          ) : (
            <input
              className={fieldClass}
              value={item[field]}
              onChange={(event) => onChange(updateText(item, field, event.target.value))}
            />
          )}
        </label>
      ))}
      <ListFields item={item} onChange={onChange} />
    </div>
  </section>
);
const ListFields = ({
  item,
  onChange,
}: Readonly<{ item: WorkItemDraft; onChange: (item: WorkItemDraft) => void }>): ReactNode => (
  <>
    {listFields.map((field) => (
      <label key={field} className="block text-xs font-medium text-slate-400">
        {field.replaceAll("_", " ")}
        <input
          className={fieldClass}
          value={item[field].join(", ")}
          onChange={(event) => onChange(updateList(item, field, event.target.value))}
        />
      </label>
    ))}
  </>
);
