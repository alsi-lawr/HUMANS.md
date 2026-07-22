import { useState } from "react";
import { type Record } from "../model";

export type BrowseTab = "projects" | "investigations" | "tickets" | "files" | "strategies";
export type Project = Readonly<{ name: string; investigations: number; tickets: number }>;
export type Investigation = Readonly<{ name: string; tickets: number }>;
export type ScopeNavigation = Readonly<{
  tab: BrowseTab;
  projects: ReadonlyArray<Project>;
  investigations: ReadonlyArray<Investigation>;
  tickets: ReadonlyArray<Record>;
  files: ReadonlyArray<Record>;
  strategies: ReadonlyArray<Record>;
  project: string | undefined;
  investigation: string | undefined;
  selectTab: (tab: BrowseTab) => void;
  selectProject: (project: string) => void;
  selectInvestigation: (investigation: string) => void;
}>;

export const useScopeNavigation = (records: ReadonlyArray<Record>): ScopeNavigation => {
  const [tab, setTab] = useState<BrowseTab>("projects");
  const [project, setProject] = useState<string | undefined>(undefined);
  const [investigation, setInvestigation] = useState<string | undefined>(undefined);
  const projects = projectRows(records);
  const investigations = investigationRows(records, project);
  const tickets = records.filter(
    (record) => matchesInvestigation(record, project, investigation) && isTicket(record),
  );
  const files = records.filter(
    (record) => matchesFiles(record, project, investigation) && !isTicket(record),
  );
  const strategies = strategiesForInvestigation(records, project, investigation);

  return {
    tab,
    projects,
    investigations,
    tickets,
    files,
    strategies,
    project,
    investigation,
    selectTab: setTab,
    selectProject: (selectedProject) => {
      setProject(selectedProject);
      setInvestigation(undefined);
      setTab("investigations");
    },
    selectInvestigation: (selectedInvestigation) => {
      setInvestigation(selectedInvestigation);
      setTab("tickets");
    },
  };
};

const projectRows = (records: ReadonlyArray<Record>): ReadonlyArray<Project> =>
  [
    ...new Set(
      records.flatMap((record) => (record.scope === undefined ? [] : [record.scope.project])),
    ),
  ]
    .sort((left, right) => left.localeCompare(right))
    .map((name) => ({
      name,
      investigations: investigationRows(records, name).length,
      tickets: records.filter((record) => record.scope?.project === name && isTicket(record))
        .length,
    }));

const investigationRows = (
  records: ReadonlyArray<Record>,
  project: string | undefined,
): ReadonlyArray<Investigation> => {
  if (project === undefined) return [];
  return [
    ...new Set(
      records.flatMap((record) =>
        record.scope?.project === project && record.scope.investigation !== undefined
          ? [record.scope.investigation]
          : [],
      ),
    ),
  ]
    .sort((left, right) => left.localeCompare(right))
    .map((name) => ({
      name,
      tickets: records.filter(
        (record) =>
          record.scope?.project === project &&
          record.scope.investigation === name &&
          isTicket(record),
      ).length,
    }));
};

const matchesInvestigation = (
  record: Record,
  project: string | undefined,
  investigation: string | undefined,
): boolean =>
  project !== undefined &&
  investigation !== undefined &&
  record.scope?.project === project &&
  record.scope.investigation === investigation;

const matchesFiles = (
  record: Record,
  project: string | undefined,
  investigation: string | undefined,
): boolean =>
  project !== undefined &&
  record.scope?.project === project &&
  (investigation === undefined ||
    record.scope.investigation === undefined ||
    record.scope.investigation === investigation);

const isTicket = (record: Record): boolean =>
  record.classification === "governed" && (record.kind === "ticket" || record.kind === "epic");

const isStrategy = (record: Record): boolean =>
  (record.classification === "governed" || record.classification === "invalid") &&
  (record.kind === "strategy" || record.kind === "strategy_binding");

export const strategiesForInvestigation = (
  records: ReadonlyArray<Record>,
  project: string | undefined,
  investigation: string | undefined,
): ReadonlyArray<Record> =>
  records.filter(
    (record) => matchesInvestigation(record, project, investigation) && isStrategy(record),
  );
