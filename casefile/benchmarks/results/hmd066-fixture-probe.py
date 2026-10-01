from pathlib import Path
import tempfile, shutil, subprocess, urllib.request, json, hashlib
ROOT = Path("/home/alex/dev/HUMANS.md")
BINARY = ROOT / ".agent-workspace/20260930-speedup-implementation/writer/target/debug/casefile"
INVESTIGATION = "projects/demo/investigations/sample"
results = {"binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(), "fixtures": []}
for count in [250, 1000]:
    with tempfile.TemporaryDirectory(prefix="hmd066-fixture-") as temporary:
        temporary = Path(temporary)
        store = temporary / "store"
        shutil.copytree(ROOT / "casefile/casefile-store/tests/fixtures/minimum", store)
        directory = store / INVESTIGATION / "tickets/accepted"
        template = (directory / "HMD-011.md").read_text()
        for number in range(count):
            identity = f"HMD-{number + 100000:06}"
            (directory / f"{identity}.md").write_text(template.replace("HMD-011", identity))
        progress = store / INVESTIGATION / "progress"
        progress.mkdir()
        (progress / "log.toml").write_text("schema_version = 1\n" + "".join(
            f'\n[[entries]]\nid = "note-{number:06}"\nrecorded_at = "2026-07-26T10:01:00Z"\nrecorded_by = "benchmark"\nticket_id = "HMD-011"\nkind = "note"\ncategory = "quirk"\nmessage = "Synthetic benchmark note."\n'
            for number in range(500)))
        process = subprocess.Popen([str(BINARY), "--root", str(store), "serve", "--index", str(temporary / "index.sqlite")], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            lines = [process.stdout.readline().strip() for _ in range(4)]
            address = lines[0].removeprefix("Casefile server: ")
            def query(body):
                request = urllib.request.Request(address + "/api/query", json.dumps(body).encode(), {"Content-Type":"application/json"})
                with urllib.request.urlopen(request, timeout=15) as response:
                    assert response.status == 200
                    return json.load(response)
            scoped = query({"query":"records","scope":{"project":"demo","investigation":"sample"}})["Current"]["value"]
            hits = query({"query":"records","scope":{"project":"demo","investigation":"sample"},"search":"HMD-100000"})["Current"]["value"]
            misses = query({"query":"records","scope":{"project":"demo","investigation":"sample"},"search":"no-such-record-fragment"})["Current"]["value"]
            first = query({"query":"workspace"})
            same = query({"query":"workspace","known_token":first["freshness"],"search":"HMD-100000"})
            assert first["state"] == "updated" and same["state"] == "unchanged"
            assert len(hits) == 1 and not misses
            assert same["matching_paths"] == [hits[0]["path"]]
            assert "records" not in same and "diagnostics" not in same
            assert all("content" in row and "rendered_markdown" in row for row in hits)
            results["fixtures"].append({"added_tickets":count,"actual_work_items":sum(row.get("kind") in ["ticket","epic"] for row in scoped),"canonical_file_count":sum(p.is_file() for p in store.rglob("*")),"scoped_record_count":len(scoped),"workspace_record_count":len(first["records"]),"progress_notes":500,"search_hit_paths":[row["path"] for row in hits],"search_miss_count":len(misses),"unchanged_fields":sorted(same),"diagnostic_count":len(first["diagnostics"])})
        finally:
            process.terminate()
            stdout, stderr = process.communicate(timeout=15)
            # Launch capability is intentionally not retained.
            assert not stderr, stderr
print(json.dumps(results, indent=2))
