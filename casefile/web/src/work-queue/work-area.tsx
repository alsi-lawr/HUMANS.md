import { type ReactNode } from "react";
import {
  type BrowseTab,
  type Investigation,
  type Project,
} from "../navigation/use-scope-navigation";
import { type Diagnostic, type Record } from "../model";
import { StrategyPanel } from "../strategy/strategy-panel";
import { Badge, classificationTone, kindTone } from "../ui/badge";

export type BrowserPanelProps = Readonly<{
  tab: BrowseTab;
  projects: ReadonlyArray<Project>;
  investigations: ReadonlyArray<Investigation>;
  tickets: ReadonlyArray<Record>;
  files: ReadonlyArray<Record>;
  strategies: ReadonlyArray<Record>;
  project: string | undefined;
  investigation: string | undefined;
  selectedPath: string | undefined;
  selectedRecord: Record | undefined;
  diagnostics: ReadonlyArray<Diagnostic>;
  search: string;
  onProject: (project: string) => void;
  onInvestigation: (investigation: string) => void;
  onSelect: (record: Record) => void;
}>;

export const BrowserPanel = ({
  tab,
  projects,
  investigations,
  tickets,
  files,
  strategies,
  project,
  investigation,
  selectedPath,
  selectedRecord,
  diagnostics,
  search,
  onProject,
  onInvestigation,
  onSelect,
}: BrowserPanelProps): ReactNode => (
  <main className="min-w-0 bg-slate-950 p-4 lg:overflow-y-auto lg:p-6">
    {tab === "projects" ? <ProjectList projects={projects} onSelect={onProject} /> : undefined}
    {tab === "investigations" ? (
      <InvestigationList
        project={project}
        investigations={investigations}
        onSelect={onInvestigation}
      />
    ) : undefined}
    {tab === "tickets" ? (
      <RecordList
        title="Tickets"
        empty="Select an investigation to inspect its governed tickets and epics."
        records={tickets}
        selectedPath={selectedPath}
        onSelect={onSelect}
      />
    ) : undefined}
    {tab === "files" ? (
      <FileList
        files={files}
        project={project}
        investigation={investigation}
        selectedPath={selectedPath}
        onSelect={onSelect}
      />
    ) : undefined}
    {tab === "strategies" ? (
      <StrategyPanel
        investigation={investigation}
        records={strategies}
        selectedRecord={strategies.find((record) => record.path === selectedRecord?.path)}
        selectedPath={selectedPath}
        diagnostics={diagnostics}
        search={search}
        onSelect={onSelect}
      />
    ) : undefined}
  </main>
);

const ProjectList = ({
  projects,
  onSelect,
}: Readonly<{
  projects: ReadonlyArray<Project>;
  onSelect: (project: string) => void;
}>): ReactNode => (
  <section>
    <header className="mb-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">Projects</p>
      <h1 className="mt-1 text-xl font-semibold text-slate-100">Casefile projects</h1>
    </header>
    {projects.length === 0 ? (
      <Empty message="No project-scoped records were returned." />
    ) : (
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {projects.map((project) => (
          <button
            key={project.name}
            type="button"
            onClick={() => onSelect(project.name)}
            className="rounded-xl border border-slate-800 bg-slate-900/50 p-4 text-left hover:border-slate-700"
          >
            <p className="text-base font-semibold text-slate-100">{project.name}</p>
            <p className="mt-2 text-sm text-slate-500">
              {project.investigations} investigations · {project.tickets} tickets
            </p>
          </button>
        ))}
      </div>
    )}
  </section>
);

const InvestigationList = ({
  project,
  investigations,
  onSelect,
}: Readonly<{
  project: string | undefined;
  investigations: ReadonlyArray<Investigation>;
  onSelect: (investigation: string) => void;
}>): ReactNode => (
  <section>
    <header className="mb-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">
        Investigations
      </p>
      <h1 className="mt-1 text-xl font-semibold text-slate-100">{project ?? "Select a project"}</h1>
    </header>
    {investigations.length === 0 ? (
      <Empty message="Select a project with investigations to continue." />
    ) : (
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {investigations.map((investigation) => (
          <button
            key={investigation.name}
            type="button"
            onClick={() => onSelect(investigation.name)}
            className="rounded-xl border border-slate-800 bg-slate-900/50 p-4 text-left hover:border-slate-700"
          >
            <p className="text-base font-semibold text-slate-100">{investigation.name}</p>
            <p className="mt-2 text-sm text-slate-500">{investigation.tickets} tickets</p>
          </button>
        ))}
      </div>
    )}
  </section>
);

const FileList = ({
  files,
  project,
  investigation,
  selectedPath,
  onSelect,
}: Readonly<{
  files: ReadonlyArray<Record>;
  project: string | undefined;
  investigation: string | undefined;
  selectedPath: string | undefined;
  onSelect: (record: Record) => void;
}>): ReactNode => (
  <section>
    <header className="mb-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">Files</p>
      <h1 className="mt-1 text-xl font-semibold text-slate-100">Files by directory</h1>
      <p className="mt-2 text-sm text-slate-500">
        {investigation === undefined
          ? "Select an investigation to include its files."
          : "Project-level files remain visible with the selected investigation."}
      </p>
    </header>
    {files.length === 0 ? (
      <Empty message="Select a project to inspect its non-ticket files." />
    ) : (
      <div className="space-y-6">
        {groupFiles(files, project, investigation).map((group) => (
          <section key={group.directory}>
            <h2 className="mb-3 font-mono text-sm font-semibold text-blue-300">
              {group.directory}/
            </h2>
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
              {group.records.map((record) => (
                <RecordCard
                  key={record.path}
                  record={record}
                  selected={record.path === selectedPath}
                  onSelect={onSelect}
                />
              ))}
            </div>
          </section>
        ))}
      </div>
    )}
  </section>
);

const RecordList = ({
  title,
  empty,
  records,
  selectedPath,
  onSelect,
}: Readonly<{
  title: string;
  empty: string;
  records: ReadonlyArray<Record>;
  selectedPath: string | undefined;
  onSelect: (record: Record) => void;
}>): ReactNode => (
  <section>
    <header className="mb-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-blue-400">{title}</p>
      <h1 className="mt-1 text-xl font-semibold text-slate-100">Governed work</h1>
    </header>
    {records.length === 0 ? (
      <Empty message={empty} />
    ) : (
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {records.map((record) => (
          <RecordCard
            key={record.path}
            record={record}
            selected={record.path === selectedPath}
            onSelect={onSelect}
          />
        ))}
      </div>
    )}
  </section>
);

const RecordCard = ({
  record,
  selected,
  onSelect,
}: Readonly<{
  record: Record;
  selected: boolean;
  onSelect: (record: Record) => void;
}>): ReactNode => (
  <button
    type="button"
    onClick={() => onSelect(record)}
    className={selected ? selectedCardClass : cardClass}
  >
    <div className="flex flex-wrap items-center gap-2">
      <Badge tone={classificationTone[record.classification]}>{record.classification}</Badge>
      {record.kind === undefined ? undefined : (
        <Badge tone={kindTone(record.kind)}>{record.kind}</Badge>
      )}
    </div>
    <p className="mt-3 line-clamp-2 text-sm font-medium text-slate-200">{record.title}</p>
    <p className="mt-2 truncate font-mono text-xs text-slate-500">{fileName(record.path)}</p>
  </button>
);

const Empty = ({ message }: Readonly<{ message: string }>): ReactNode => (
  <p className="rounded-xl border border-dashed border-slate-800 p-8 text-sm text-slate-500">
    {message}
  </p>
);

type FileGroup = Readonly<{ directory: string; records: ReadonlyArray<Record> }>;

const groupFiles = (
  files: ReadonlyArray<Record>,
  project: string | undefined,
  investigation: string | undefined,
): ReadonlyArray<FileGroup> => {
  const groups = new Map<string, Array<Record>>();
  for (const record of files) {
    const directory = relativeDirectory(
      record.path,
      project,
      investigation,
      record.scope?.investigation,
    );
    const records = groups.get(directory) ?? [];
    records.push(record);
    groups.set(directory, records);
  }
  return [...groups.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([directory, records]) => ({
      directory,
      records: [...records].sort((left, right) => left.path.localeCompare(right.path)),
    }));
};

const relativeDirectory = (
  path: string,
  project: string | undefined,
  investigation: string | undefined,
  recordInvestigation: string | undefined,
): string => {
  const projectPrefix = project === undefined ? "" : `projects/${project}/`;
  const investigationPrefix =
    investigation === undefined ? "" : `projects/${project}/investigations/${investigation}/`;
  const prefix = recordInvestigation === undefined ? projectPrefix : investigationPrefix;
  const relative =
    prefix === "" ? path : path.startsWith(prefix) ? path.slice(prefix.length) : path;
  const index = relative.lastIndexOf("/");
  return index === -1 ? "." : relative.slice(0, index);
};

const fileName = (path: string): string => path.split("/").at(-1) ?? path;
const cardClass =
  "rounded-xl border border-slate-800 bg-slate-900/50 p-4 text-left hover:border-slate-700";
const selectedCardClass = "rounded-xl border border-blue-500 bg-blue-500/10 p-4 text-left";
