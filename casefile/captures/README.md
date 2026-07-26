# Casefile documentation captures

These sources generate the Casefile wiki's committed PNG stills and terminal WebP from a disposable
public Store. They contain no user configuration, centralized planning data, or secrets.

## Provenance

| Input            | Pinned identity                                                                                                                      |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Casefile         | Published annotated tag `v0.3.4`, commit `7cd49f04aacc34f3f7b27d60aa0c2ee3f771c5e7`, tree `619e71f0cb34839d3d2e8898fc193e25e53ef18a` |
| Browser capture  | [Viset](https://github.com/getviset/Viset) `370ef7b656378487486a498589cac6419cfcd861`                                                |
| Terminal capture | [VHS fork](https://github.com/alsi-lawr/vhs) `bb4e27a982f4f126b3c71bbab8cbb08bad02002a`                                              |

`fixture/demo-store/` is a template without a progress log. `prepare-demo-store.sh` copies it into
task scratch, bootstraps progress, and records the visible states only through
`transition-ticket-progress.py`. The scripts intentionally reject a release or tool revision
mismatch.

## Regenerate

Run from a clean current HUMANS.md checkout containing these scripts. Supply separate clean
checkouts for the published v0.3.4 source, the HUMANS.md wiki, and the pinned Viset and VHS
revisions. Keep every build, browser profile, index, preview, and intermediate output in task
scratch, not `/tmp`:

```sh
./casefile/captures/run.sh \
  /path/to/HUMANS.md-v0.3.4 \
  /path/to/HUMANS.md.wiki \
  /path/to/Viset \
  /path/to/vhs \
  /path/to/HUMANS.md-v0.3.4/.agent-workspace/wiki-captures
```

The runner builds the CLI from the checked `v0.3.4` source, validates the fixture, captures the
browser Board with Viset, and records the terminal TUI with VHS's synchronized `record` mode. It
copies only the reviewed media into the supplied wiki checkout's `assets/casefile/` directory.
