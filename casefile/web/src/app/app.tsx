import { type ReactNode } from "react";
import { Sidebar } from "../navigation/sidebar";
import { useScopeNavigation } from "../navigation/use-scope-navigation";
import { DetailPanel } from "../record-detail/record-detail";
import { useChangeReview } from "../record-detail/use-change-review";
import { useRecordSelection } from "../record-detail/use-record-selection";
import { BrowserPanel } from "../work-queue/work-area";
import { Failure, Loading, Topbar } from "./app-shell";
import { useWorkspace } from "./use-workspace";
import { useBoards } from "../boards/use-boards";
import { type Draft, type Record } from "../model";

export const App = (): ReactNode => {
  const workspace = useWorkspace();
  const allRecords = workspace.workspace.tag === "ready" ? workspace.workspace.records : [];
  const unfilteredRecords =
    workspace.workspace.tag === "ready" ? workspace.workspace.unfilteredRecords : [];
  const navigation = useScopeNavigation(allRecords);
  const selection = useRecordSelection(unfilteredRecords);
  const changes = useChangeReview();
  const boards = useBoards(
    navigation.project === undefined || navigation.investigation === undefined
      ? undefined
      : { project: navigation.project, investigation: navigation.investigation },
    workspace.workspace.tag === "ready" ? workspace.workspace.sourceRevision : undefined,
    workspace.refreshKey,
    workspace.workspace.tag === "ready" ? workspace.workspace.diagnostics : [],
  );

  if (workspace.workspace.tag === "loading") return <Loading />;
  if (workspace.workspace.tag === "failure")
    return <Failure message={workspace.workspace.message} onRetry={workspace.refresh} />;

  const selectRecord = (record: Record): void => {
    selection.selectRecord(record);
    changes.reset();
  };
  const selectProject = (project: string): void => {
    navigation.selectProject(project);
    selection.clearRecord();
    changes.reset();
  };
  const selectInvestigation = (investigation: string): void => {
    navigation.selectInvestigation(
      investigation,
      navigation.tab === "boards" ? "boards" : undefined,
    );
    selection.clearRecord();
    changes.reset();
  };
  const updateDraft = (draft: Draft): void => {
    selection.updateDraft(draft);
    changes.draftChanged();
  };
  const queryError = selection.error;
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
          tab={navigation.tab}
          projects={navigation.projects}
          investigations={navigation.investigations}
          project={navigation.project}
          investigation={navigation.investigation}
          diagnostics={workspace.workspace.diagnostics}
          onTab={navigation.selectTab}
          onProject={selectProject}
          onInvestigation={selectInvestigation}
        />
        <BrowserPanel
          tab={navigation.tab}
          projects={navigation.projects}
          investigations={navigation.investigations}
          tickets={navigation.tickets}
          files={navigation.files}
          strategies={navigation.strategies}
          project={navigation.project}
          investigation={navigation.investigation}
          selectedPath={selection.selectedPath}
          selectedRecord={selection.record}
          diagnostics={workspace.workspace.diagnostics}
          search={workspace.search}
          boards={boards}
          boardRecords={unfilteredRecords}
          onProject={selectProject}
          onInvestigation={selectInvestigation}
          onSelect={selectRecord}
        />
        <DetailPanel
          record={selection.record}
          diagnostics={workspace.workspace.diagnostics}
          relationships={selection.relationships}
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
