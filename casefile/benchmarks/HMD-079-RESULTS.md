# HMD-079 installed Codex role selection

## Environment

The tested product source was `781bacd01bae3942635e735f92860c890ba6bed0`. Codex CLI 0.170.0
installed the manifest-built Casefile 0.10.0 plugin from a local marketplace in separate disposable
V1 and V2 homes. The Linux x64 runtime was built from that source, with SHA-256
`7abd715e412b58c0c7a43d50cf471015bcbf8a6671569c04a724c9347b852eb6`. The package used that local
executable for setup and MCP, and fixture binaries for the unused targets.

A loopback provider served live `/v1/models` metadata and scripted Responses events. The harness
supplied the acceptance and edit decisions. The actual Codex executables loaded the roles,
dispatched the tool calls, and issued the root and child requests. The fixture exposed the direct
native tool surface. The walkthrough covers local orchestration and leaves remote inference, account
eligibility, other host platforms, and nested or deferred launches outside its observations.

## Selection and launch

The installed executable negotiated MCP `2025-11-25` on the disposable Git planning Store. The
harness selected the packaged matrices through strategy previews and applies. It used the scoped
role projection, live `offer-role-models.py`, and `recommend-role-assignments.py recommend` and
`select`. The offer used the caller's actual native role and fork fields, provider metadata, and
previously observed Sol/high and Sol/medium override pairs on the same runtime. The native
string-valued override schema supplied no exhaustive model allowlist.

The parent was `gpt-6.2-astra` at high effort in both homes. Each row below ran once in V1 and once
in V2. The actual child request included the selected model and effort and an identifying prefix
from the installed role's instructions. The new child `turn_context` corroborated the pair. The
fixture passed the resolver's spawn fields unchanged, adding only the task input and a task name
where required. V1 used `fork_context = false` and V2 used `fork_turns = "none"`.

| Strategy                    | Role launched                                           | Confirmation                                                                                               | Observed child in V1 and V2 |
| --------------------------- | ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- | --------------------------- |
| Atomic investigation        | `casefile-investigate-atomic-detective`                 | Accepted Sol/medium as the labeled alternative to Terra/medium                                             | `gpt-6.2-sol` / medium      |
| Two-stage review            | `casefile-review-two-stage-atomic-ticket-reviewer`      | Edited the suggested Sol/high row to Sol/medium and confirmed the set with the verifier's Sol/medium row   | `gpt-6.2-sol` / medium      |
| Ticket-batch implementation | `casefile-implement-ticket-batch-implementation-writer` | Accepted the complete set with Sol/high for the writer and atomic reviewer and Sol/medium for the verifier | `gpt-6.2-sol` / high        |

The previews left each assignment file absent until apply. Reprojection returned the exact confirmed
rows, with `strategy_assignment` as their source. Re-entry with those pairs eligible returned
`reuse` with zero confirmation actions. Ordinary selection and resolver calls used the canonical
role assignment projection. The historical writer-binding display projection and matrix-default
presentation were outside the launch path.

- **Unselected and unavailable.** In V1, an investigation offer produced a recommendation while the
  resolver blocked the unconfirmed detective. With Sol withdrawn, both V1 review roles were
  unassigned. The visible parent model lacked verified named-role eligibility in this fixture. An
  attempted selection failed, the assignment files stayed absent, and both runs produced zero child
  requests.
- **Writer progress.** Both writer resolvers refused before canonical ticket progress existed. The
  fixture bootstrapped progress and applied `in_progress` for the exact accepted ticket before the
  successful launches.
- **Resume.** The actual V2 parent thread resumed using `codex exec resume` after the provider
  withdrew its saved Sol model. Fresh model discovery and revalidation refused the saved Sol/high
  writer. The run issued zero child requests and retained the assignment file byte-for-byte.
- **Solo.** Both selected solo matrices projected zero roles. The recommendation helper returned
  `skip` with zero actions. This step issued zero Casefile model-list requests and wrote zero
  assignment files.

## Install, upgrade, and uninstall

Fresh installs created schema-6 receipts and registered the native executable and named roles. Both
homes started and ended with their independent provider, MCP entry, and custom agent intact, with
zero Casefile catalog files and an absent catalog override. Native `model/list` reached the loopback
provider before setup, during the walkthrough, and after uninstall. The V1 provider still advertised
both fixture models. V2 still advertised only Astra after the withdrawal. All three confirmed
assignment files per home survived uninstall byte-for-byte.

Two separate legacy homes began with the published Casefile 0.10.0 plugin installed by Codex CLI
0.159.1, a genuine V2 schema-6 receipt, and an explicit `gpt-5.6-sol` / high writer binding applied
through that installed executable. The current manifest-built payload replaced the old package in
each disposable cache at the same package version. Codex 0.170.0 ran upgrade, reinstall, and
uninstall. Both upgrades kept active V2, archived the retired receipt-owned V2 catalog in the
rollback snapshot, retained the untracked V1-named file and host settings, and rejected uninstall
with the superseded receipt. Each retained the old matrix and explicit binding bytes. The current
canonical projection exposed the historical pair as `legacy_writer_binding` for the ordinary
ticket-batch writer registration. The binding SHA-256 was
`751bf16dade5057035d012ca2f8859bee321716c0eb7d7a03af79c83677039f3`.

In the first legacy home, the upgrade restored the pre-install foreign catalog setting and preserved
its file bytes. Native `model/list` respected that host override through upgrade, reinstall, and
uninstall. In the second home, the host removed the old global override before upgrade. It stayed
absent and each stage queried the configured `/v1/models` endpoint, returning both advertised
fixture models. Uninstall preserved the foreign file, the host-selected provider, and the historical
binding in both homes. Native `codex plugin list` confirmed plugin removal in all four homes. Their
configs contained zero remaining Casefile role registrations.
