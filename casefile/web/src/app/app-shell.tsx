import { type ReactNode } from "react";

export const Topbar = ({
  capability,
  search,
  onSearch,
  onRefresh,
}: Readonly<{
  capability: string;
  search: string;
  onSearch: (value: string) => void;
  onRefresh: () => void;
}>): ReactNode => (
  <header className="flex items-center justify-between gap-4 border-b border-slate-800 bg-slate-950 px-4 py-3 lg:px-6">
    <div className="flex min-w-0 items-center gap-3">
      <span className="grid h-8 w-8 place-items-center rounded-lg bg-blue-500 font-bold text-white">
        C
      </span>
      <div>
        <p className="text-sm font-semibold text-slate-100">Casefile</p>
        <p className="text-xs text-slate-500">Planning workbench</p>
      </div>
    </div>
    <form
      className="hidden min-w-0 max-w-xl flex-1 md:block"
      onSubmit={(event) => {
        event.preventDefault();
        onRefresh();
      }}
    >
      <label className="sr-only" htmlFor="workbench-search">
        Search records
      </label>
      <input
        id="workbench-search"
        value={search}
        onChange={(event) => onSearch(event.target.value)}
        placeholder="Search records"
        className="w-full rounded-lg border border-slate-800 bg-slate-900 px-3 py-2 text-sm text-slate-200 outline-none placeholder:text-slate-600 focus:border-blue-400"
      />
    </form>
    <div className="flex items-center gap-3">
      <span
        className={`hidden text-xs sm:inline ${capability.length === 0 ? "text-slate-500" : "text-emerald-300"}`}
      >
        {capability.length === 0 ? "Read-only" : "Write capability loaded"}
      </span>
      <button
        type="button"
        onClick={onRefresh}
        className="rounded-lg border border-slate-700 px-3 py-2 text-sm font-medium text-slate-200 hover:border-blue-400 hover:text-blue-200"
      >
        Refresh
      </button>
    </div>
  </header>
);
export const Loading = (): ReactNode => (
  <div className="grid min-h-screen place-items-center bg-slate-950 text-slate-400">
    <p>Refreshing Casefile index…</p>
  </div>
);
export const Failure = ({
  message,
  onRetry,
}: Readonly<{ message: string; onRetry: () => void }>): ReactNode => (
  <div className="grid min-h-screen place-items-center bg-slate-950 p-6">
    <section className="max-w-md rounded-xl border border-rose-500/30 bg-rose-500/10 p-6">
      <p className="text-sm font-semibold text-rose-200">The workbench could not load.</p>
      <p className="mt-2 text-sm leading-6 text-slate-300">{message}</p>
      <button
        type="button"
        onClick={onRetry}
        className="mt-5 rounded-lg bg-blue-500 px-3 py-2 text-sm font-semibold text-white hover:bg-blue-400"
      >
        Try again
      </button>
    </section>
  </div>
);
