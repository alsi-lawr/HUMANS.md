import { type ReactNode } from "react";
import { type Draft } from "../model";
import { BoardEditor } from "./board-editor";
import { WorkItemEditor } from "./work-item-editor";

export const Editor = ({
  draft,
  onChange,
}: Readonly<{ draft: Draft; onChange: (draft: Draft) => void }>): ReactNode =>
  draft.kind === "board" ? (
    <BoardEditor board={draft.value} onChange={(value) => onChange({ kind: "board", value })} />
  ) : (
    <WorkItemEditor
      kind={draft.kind}
      item={draft.value}
      onChange={(value) => onChange({ kind: draft.kind, value })}
    />
  );
