import { type ReactNode } from "react";
import { type Preview } from "../model";

export type MutationStatus = "idle" | "previewing" | "applying" | "error" | "conflict" | "applied";

const fieldClass =
  "mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 py-2 text-sm text-slate-100 outline-none placeholder:text-slate-600 focus:border-blue-400";
const messageClasses = {
  idle: "text-sm text-emerald-300",
  previewing: "text-sm text-emerald-300",
  applying: "text-sm text-emerald-300",
  error: "text-sm text-rose-300",
  conflict: "text-sm text-amber-200",
  applied: "text-sm text-emerald-300",
} satisfies Readonly<{ [Status in MutationStatus]: string }>;

export const ChangeControls = ({
  capability,
  status,
  message,
  onCapability,
  onPreview,
  onApply,
  onReconcile,
  preview,
}: Readonly<{
  capability: string;
  status: MutationStatus;
  message: string | undefined;
  onCapability: (value: string) => void;
  onPreview: () => void;
  onApply: () => void;
  onReconcile: () => void;
  preview: Preview | undefined;
}>): ReactNode => (
  <section className="space-y-3 border-t border-slate-800 pt-5">
    <label className="block text-xs font-semibold uppercase tracking-widest text-slate-500">
      Write capability
      <input
        aria-label="Write capability"
        className={fieldClass}
        type="password"
        autoComplete="off"
        value={capability}
        onChange={(event) => onCapability(event.target.value)}
        placeholder="Paste capability from the launch terminal"
      />
    </label>
    <p className="text-xs leading-5 text-slate-500">
      Held only in this page’s memory and sent only when applying a reviewed preview.
    </p>
    <div className="flex gap-2">
      <button
        type="button"
        disabled={status === "previewing" || status === "applying" || status === "conflict"}
        onClick={onPreview}
        className="rounded-lg bg-blue-500 px-3 py-2 text-sm font-semibold text-white hover:bg-blue-400 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {status === "previewing" ? "Preparing…" : "Preview changes"}
      </button>
      {preview === undefined ? undefined : (
        <button
          type="button"
          disabled={
            capability.length === 0 || status === "applying" || preview.diagnostics.length > 0
          }
          onClick={onApply}
          className="rounded-lg border border-emerald-500/50 bg-emerald-500/10 px-3 py-2 text-sm font-semibold text-emerald-200 hover:bg-emerald-500/20 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {status === "applying" ? "Applying…" : "Apply preview"}
        </button>
      )}
      {status === "conflict" ? (
        <button
          type="button"
          onClick={onReconcile}
          className="rounded-lg border border-amber-500/50 bg-amber-500/10 px-3 py-2 text-sm font-semibold text-amber-200 hover:bg-amber-500/20"
        >
          Resume after reconciliation
        </button>
      ) : undefined}
    </div>
    {message === undefined ? undefined : (
      <p role="status" className={messageClasses[status]}>
        {message}
      </p>
    )}
    {preview === undefined ? undefined : <PreviewView preview={preview} />}
  </section>
);
const PreviewView = ({ preview }: Readonly<{ preview: Preview }>): ReactNode => (
  <div className="space-y-3">
    <div className="rounded-lg border border-slate-800 bg-slate-900/50 p-3">
      <p className="text-xs font-semibold uppercase tracking-widest text-slate-500">
        Preview diagnostics
      </p>
      {preview.diagnostics.length === 0 ? (
        <p className="mt-2 text-sm text-emerald-300">No host diagnostics.</p>
      ) : (
        <ul className="mt-2 space-y-2">
          {preview.diagnostics.map((diagnostic) => (
            <li
              key={`${diagnostic.path}:${diagnostic.code}:${diagnostic.message}`}
              className="text-sm text-amber-200"
            >
              {diagnostic.code}: {diagnostic.message}
            </li>
          ))}
        </ul>
      )}
    </div>
    <div>
      <p className="mb-2 text-xs font-semibold uppercase tracking-widest text-slate-500">
        Exact diff
      </p>
      <pre className="max-h-96 overflow-auto rounded-lg border border-slate-800 bg-slate-950 p-3 font-mono text-xs leading-5 text-slate-300">
        {preview.diff}
      </pre>
    </div>
  </div>
);
