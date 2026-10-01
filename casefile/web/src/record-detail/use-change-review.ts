import { useState } from "react";
import { apply, preview } from "../api";
import {
  type Draft,
  type Preview,
  type Record,
  type WorkspaceContext,
  type WorkspaceResponse,
  toChangeRequest,
} from "../model";
import { type MutationStatus } from "./change-review";

export type ChangeReview = Readonly<{
  preview: Preview | undefined;
  capability: string;
  status: MutationStatus;
  message: string | undefined;
  setCapability: (value: string) => void;
  reset: () => void;
  draftChanged: () => void;
  resolveConflict: () => void;
  prepare: (record: Record | undefined, draft: Draft | undefined) => void;
  apply: (options: ApplyReviewOptions) => void;
}>;
type ApplyReviewOptions = Readonly<{
  context: WorkspaceContext;
  completed: (value: WorkspaceResponse, context: WorkspaceContext) => void;
  refresh: () => void;
}>;
type Mutation =
  | Readonly<{ tag: "idle" }>
  | Readonly<{ tag: "previewing" }>
  | Readonly<{ tag: "applying" }>
  | Readonly<{ tag: "error"; message: string }>
  | Readonly<{ tag: "conflict"; message: string }>
  | Readonly<{ tag: "applied"; message: string }>;

export const useChangeReview = (): ChangeReview => {
  const [review, setReview] = useState<Preview | undefined>(undefined);
  const [capability, setCapability] = useState("");
  const [mutation, setMutation] = useState<Mutation>({ tag: "idle" });

  const reset = (): void => {
    setReview(undefined);
    setMutation({ tag: "idle" });
  };
  const draftChanged = (): void => {
    setReview(undefined);
    setMutation((current) => (current.tag === "conflict" ? current : { tag: "idle" }));
  };

  const prepare = (record: Record | undefined, draft: Draft | undefined): void => {
    if (record === undefined || draft === undefined) return;
    const controller = new AbortController();
    setMutation({ tag: "previewing" });
    void preview(toChangeRequest(record.path, draft), controller.signal).then((result) => {
      if (result.tag === "failure") {
        setMutation({ tag: "error", message: result.message });
        return;
      }
      setReview(result.value);
      setMutation({ tag: "idle" });
    });
  };

  const applyReview = (options: ApplyReviewOptions): void => {
    if (review === undefined || capability.length === 0) return;
    const controller = new AbortController();
    setMutation({ tag: "applying" });
    void apply(review, { capability, signal: controller.signal, context: options.context }).then(
      (result) => {
        if (result.tag === "failure") {
          if (result.code === "stale_revision") {
            setReview(undefined);
            setMutation({
              tag: "conflict",
              message:
                "Canonical content changed. The current record was reloaded; reconcile the preserved draft before continuing.",
            });
            options.refresh();
            return;
          }
          const message =
            result.code === "incomplete_rollback"
              ? `${result.message}: ${result.details.affected_paths.map((path) => `${path.path} (${path.remaining.state}, ${path.reason})`).join("; ")}`
              : result.message;
          setMutation({ tag: "error", message });
          if (result.code === "incomplete_rollback") options.refresh();
          return;
        }
        const message =
          result.value.cache.state !== "degraded"
            ? "Applied. The work queue will refresh."
            : `Applied, but provider cache refresh reported: ${result.value.cache.message ?? "unknown error"}`;
        setMutation({ tag: "applied", message });
        if (result.value.workspace.tag === "available")
          options.completed(result.value.workspace.value, options.context);
        else options.refresh();
      },
    );
  };

  return {
    preview: review,
    capability,
    status: mutation.tag,
    message:
      mutation.tag === "error" || mutation.tag === "conflict" || mutation.tag === "applied"
        ? mutation.message
        : undefined,
    setCapability,
    reset,
    draftChanged,
    resolveConflict: () => setMutation({ tag: "idle" }),
    prepare,
    apply: applyReview,
  };
};
