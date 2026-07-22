# Project Map

Every durable planning store contains a root-level `projects.toml` that maps each project namespace
to its absolute source directory:

```toml
[projects]
"<project name>" = "<absolute directory>"
```

The project name is the namespace used at `projects/<project name>/`. Before adding that project
directory or any records beneath it, the root adds its mapping and verifies that the source
directory exists. Agents resolve project source directories from this map rather than inferring
sibling paths or relying on the current working directory.

Do not silently overwrite an existing name with a different directory. Surface the conflict to the
human and update the mapping only with explicit authorisation.

After changing the map or adding a project namespace, run
`<workflow-package>/scripts/validate-project-map.py --planning-store <planning-store>`. Do not
continue while validation fails.
