import { type ReactNode } from "react";
import { type Diagnostic, type Scope, scopeLabel } from "../model";

export type SidebarProps = Readonly<{
  scopes: ReadonlyArray<Scope>;
  selected: Scope | undefined;
  diagnostics: ReadonlyArray<Diagnostic>;
  onSelect: (scope: Scope | undefined) => void;
}>;
export const Sidebar = ({ scopes, selected, diagnostics, onSelect }: SidebarProps): ReactNode => (
  <aside className="flex min-h-0 flex-col border-r border-slate-800 bg-slate-950/80">
    <div className="border-b border-slate-800 px-4 py-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-slate-500">
        Planning space
      </p>
      <button
        type="button"
        onClick={() => onSelect(undefined)}
        className={`mt-3 flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm ${selected === undefined ? "bg-blue-500/15 text-blue-200" : "text-slate-300 hover:bg-slate-900"}`}
      >
        <span>All investigations</span>
        <span className="text-xs text-slate-500">{scopes.length}</span>
      </button>
    </div>
    <nav aria-label="Projects and investigations" className="min-h-0 flex-1 overflow-y-auto p-3">
      {scopes.length === 0 ? (
        <p className="px-2 py-4 text-sm text-slate-500">No governed scopes were returned.</p>
      ) : (
        scopes.map((scope) => {
          const active =
            selected !== undefined &&
            selected.project === scope.project &&
            selected.investigation === scope.investigation;
          return (
            <button
              key={`${scope.project}/${scope.investigation ?? ""}`}
              type="button"
              onClick={() => onSelect(scope)}
              className={`mb-1 flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm ${active ? "bg-blue-500/15 text-blue-200" : "text-slate-400 hover:bg-slate-900 hover:text-slate-200"}`}
            >
              <span className="h-2 w-2 rounded-full bg-slate-600" />
              <span className="truncate">{scopeLabel(scope)}</span>
            </button>
          );
        })
      )}
    </nav>
    <div className="border-t border-slate-800 p-4">
      <p className="text-xs font-semibold uppercase tracking-widest text-slate-500">Diagnostics</p>
      <p className="mt-2 text-sm text-slate-300">
        {diagnostics.length === 0
          ? "Clear"
          : `${diagnostics.length} item${diagnostics.length === 1 ? "" : "s"}`}
      </p>
      {diagnostics.length === 0 ? undefined : (
        <ul className="mt-3 space-y-2">
          {diagnostics.slice(0, 3).map((diagnostic) => (
            <li
              key={`${diagnostic.path}:${diagnostic.code}`}
              className="text-xs leading-5 text-amber-200"
            >
              <span className="font-mono text-amber-300">{diagnostic.code}</span>
              <span className="block truncate text-slate-500">{diagnostic.path}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  </aside>
);
