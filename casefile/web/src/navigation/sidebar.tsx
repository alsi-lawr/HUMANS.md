import { type ReactNode } from "react";
import { type Diagnostic } from "../model";
import { type BrowseTab, type Investigation, type Project } from "./use-scope-navigation";

export type SidebarProps = Readonly<{
  tab: BrowseTab;
  projects: ReadonlyArray<Project>;
  investigations: ReadonlyArray<Investigation>;
  project: string | undefined;
  investigation: string | undefined;
  diagnostics: ReadonlyArray<Diagnostic>;
  onTab: (tab: BrowseTab) => void;
  onProject: (project: string) => void;
  onInvestigation: (investigation: string) => void;
}>;

const tabs: ReadonlyArray<Readonly<{ id: BrowseTab; label: string }>> = [
  { id: "projects", label: "Projects" },
  { id: "investigations", label: "Investigations" },
  { id: "tickets", label: "Tickets" },
  { id: "files", label: "Files" },
  { id: "strategies", label: "Strategies" },
  { id: "boards", label: "Boards" },
];

export const Sidebar = ({
  tab,
  projects,
  investigations,
  project,
  investigation,
  diagnostics,
  onTab,
  onProject,
  onInvestigation,
}: SidebarProps): ReactNode => (
  <aside className="flex min-h-0 flex-col border-r border-slate-800 bg-slate-950/80">
    <div className="border-b border-slate-800 px-4 py-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-slate-500">Casefile</p>
      <nav aria-label="Casefile browsing tabs" className="mt-3 grid grid-cols-2 gap-1">
        {tabs.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            onClick={() => onTab(id)}
            className={tab === id ? activeTabClass : inactiveTabClass}
          >
            {label}
          </button>
        ))}
      </nav>
    </div>
    <nav aria-label="Projects and investigations" className="min-h-0 flex-1 overflow-y-auto p-3">
      <p className="px-2 pb-2 text-xs font-semibold uppercase tracking-widest text-slate-500">
        Projects
      </p>
      {projects.map((item) => (
        <button
          key={item.name}
          type="button"
          onClick={() => onProject(item.name)}
          className={project === item.name ? activeItemClass : inactiveItemClass}
        >
          <span className="truncate">{item.name}</span>
          <span className="text-xs text-slate-500">{item.investigations} inv</span>
        </button>
      ))}
      {project === undefined ? undefined : (
        <>
          <p className="px-2 pb-2 pt-5 text-xs font-semibold uppercase tracking-widest text-slate-500">
            Investigations
          </p>
          {investigations.map((item) => (
            <button
              key={`${project}/${item.name}`}
              type="button"
              onClick={() => onInvestigation(item.name)}
              className={investigation === item.name ? activeItemClass : inactiveItemClass}
            >
              <span className="truncate">{item.name}</span>
              <span className="text-xs text-slate-500">{item.tickets} tickets</span>
            </button>
          ))}
        </>
      )}
    </nav>
    <div className="border-t border-slate-800 p-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-slate-500">Diagnostics</p>
      <p className="mt-2 text-sm text-slate-300">
        {diagnostics.length === 0
          ? "Clear"
          : `${diagnostics.length} item${diagnostics.length === 1 ? "" : "s"}`}
      </p>
    </div>
  </aside>
);

const activeTabClass =
  "rounded-lg bg-blue-500/15 px-2 py-2 text-left text-xs font-medium text-blue-200";
const inactiveTabClass =
  "rounded-lg px-2 py-2 text-left text-xs font-medium text-slate-400 hover:bg-slate-900 hover:text-slate-200";
const activeItemClass =
  "mb-1 flex w-full items-center justify-between gap-2 rounded-lg bg-blue-500/15 px-3 py-2 text-left text-sm text-blue-200";
const inactiveItemClass =
  "mb-1 flex w-full items-center justify-between gap-2 rounded-lg px-3 py-2 text-left text-sm text-slate-400 hover:bg-slate-900 hover:text-slate-200";
