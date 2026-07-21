import { type ReactNode } from "react";
import { type Classification, type Identity, type Kind } from "../model";

type Tone = "slate" | "blue" | "amber" | "emerald" | "rose" | "violet";

const badgeClasses: Readonly<Record<Tone, string>> = {
  slate: "border-slate-700 bg-slate-800 text-slate-300",
  blue: "border-blue-500/30 bg-blue-500/10 text-blue-300",
  amber: "border-amber-500/30 bg-amber-500/10 text-amber-300",
  emerald: "border-emerald-500/30 bg-emerald-500/10 text-emerald-300",
  rose: "border-rose-500/30 bg-rose-500/10 text-rose-300",
  violet: "border-violet-500/30 bg-violet-500/10 text-violet-300",
};

export const Badge = ({
  children,
  tone,
}: Readonly<{ children: string; tone: Tone }>): ReactNode => (
  <span
    className={`inline-flex items-center rounded border px-2 py-0.5 text-xs font-semibold uppercase tracking-wide ${badgeClasses[tone]}`}
  >
    {children}
  </span>
);

export const classificationTone: Readonly<Record<Classification, Tone>> = {
  governed: "emerald",
  ungoverned: "amber",
  invalid: "rose",
  raw: "slate",
};

export const kindTone = (kind: Kind | undefined): Tone =>
  kind === "ticket" ? "blue" : kind === "epic" ? "violet" : kind === "board" ? "emerald" : "slate";

export const identityKey = (identity: Identity): string =>
  `${identity.scope.project}/${identity.scope.investigation ?? ""}/${identity.identity}`;
