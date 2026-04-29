# Claude Code Instructions

## After Making Changes

After any code change, always verify both of the following before reporting the task as complete:

1. **Clean build** — `cargo build` must succeed with no errors
2. **No clippy warnings** — `cargo clippy -- -D warnings` must pass with no warnings

If clippy reports warnings, fix them before finishing.
