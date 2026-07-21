# Codex GitHub CLI policy

Every `gh` command, including read-only commands, must request elevated or unsandboxed execution on
its first attempt. Never try the command sandboxed first or use a sandbox failure as the reason to
elevate. When the runtime requires one, include an operation-specific user-facing justification with
the elevation request.
