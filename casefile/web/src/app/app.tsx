import { type ReactNode } from "react";
import { Sidebar } from "../navigation/sidebar";
import { useScopeNavigation } from "../navigation/use-scope-navigation";
import { DetailPanel } from "../record-detail/record-detail";
import { useChangeReview } from "../record-detail/use-change-review";
import { useRecordSelection } from "../record-detail/use-record-selection";
import { BoardPanel } from "../work-queue/work-area";
import { Failure, Loading, Topbar } from "./app-shell";
import { useWorkspace } from "./use-workspace";
import { type Draft, type Identity, type Scope } from "../model";

export const App = (): ReactNode => {
  const workspace = useWorkspace();
  const allRecords = workspace.workspace.tag === "ready" ? workspace.workspace.records : [];
  const navigation = useScopeNavigation(allRecords);
  const selection = useRecordSelection(allRecords);
  const changes = useChangeReview();

  if (workspace.workspace.tag === "loading") return <Loading />;
  if (workspace.workspace.tag === "failure")
    return <Failure message={workspace.workspace.message} onRetry={workspace.refresh} />;

  const selectScope = (scope: Scope | undefined): void => {
    navigation.selectScope(scope);
    selection.clearRecord();
    changes.reset();
  };
  const selectRecord = (identity: Identity): void => {
    selection.selectRecord(identity);
    changes.reset();
  };
  const updateDraft = (draft: Draft): void => {
    selection.updateDraft(draft);
    changes.draftChanged();
  };
  const queryError = navigation.error ?? selection.error;
  const status = queryError === undefined ? changes.status : "error";
  const message = queryError ?? changes.message;

  return (
    <div className="grid h-screen grid-rows-[auto_1fr] overflow-hidden bg-slate-950 text-slate-100">
      <Topbar
        capability={changes.capability}
        search={workspace.search}
        onSearch={workspace.setSearch}
        onRefresh={workspace.refresh}
      />
      <div className="grid min-h-0 grid-cols-1 overflow-y-auto lg:grid-cols-[15rem_minmax(0,1fr)_24rem] lg:overflow-hidden">
        <Sidebar
          scopes={navigation.scopes}
          selected={navigation.scope}
          diagnostics={workspace.workspace.diagnostics}
          onSelect={selectScope}
        />
        <BoardPanel
          boards={navigation.boards}
          records={navigation.records}
          selected={selection.selected}
          onSelect={selectRecord}
        />
        <DetailPanel
          record={selection.record}
          relationships={selection.relationships}
          boards={navigation.boards}
          draft={selection.draft}
          preview={changes.preview}
          capability={changes.capability}
          status={status}
          message={message}
          onCapability={changes.setCapability}
          onDraft={updateDraft}
          onPreview={() => changes.prepare(selection.record, selection.draft)}
          onApply={() => changes.apply(workspace.refresh)}
          onReconcile={changes.resolveConflict}
        />
      </div>
    </div>
  );
};
