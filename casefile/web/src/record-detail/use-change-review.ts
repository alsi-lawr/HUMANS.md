import { useState } from "react";
import { apply, preview } from "../api";
import { type Draft, type Preview, type Record, toChangeRequest } from "../model";
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
  apply: (refresh: () => void) => void;
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

  const applyReview = (refresh: () => void): void => {
    if (review === undefined || capability.length === 0) return;
    const controller = new AbortController();
    setMutation({ tag: "applying" });
    void apply(review, capability, controller.signal).then((result) => {
      if (result.tag === "failure") {
        if (result.code === "stale_revision") {
          setReview(undefined);
          setMutation({
            tag: "conflict",
            message:
              "Canonical content changed. The current record was reloaded; reconcile the preserved draft before continuing.",
          });
          refresh();
          return;
        }
        setMutation({ tag: "error", message: result.message });
        return;
      }
      const message =
        result.value.cache.state !== "degraded"
          ? "Applied. The work queue will refresh."
          : `Applied, but provider cache refresh reported: ${result.value.cache.message ?? "unknown error"}`;
      setMutation({ tag: "applied", message });
      refresh();
    });
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
