# Contributing to Deadlock RPC

## How to Contribute

### Reporting Bugs

Check existing issues before opening a new one. A good bug report includes:
- Steps to reproduce
- Expected vs actual behavior
- OS and Deadlock RPC version

### Suggesting Features

Open an issue describing the problem you're trying to solve and your proposed solution.

### Submitting Code

Look for issues labeled `good first issue` or `help wanted` as a starting point.

## Development Setup

**Requirements:** [Rust](https://rustup.rs) stable

```bash
git clone https://github.com/tariq-swe/deadlock-rpc.git
cd deadlock-rpc

# Create a branch
git checkout -b feature/your-feature-name

# Build
cargo build --release

# Run
./target/release/deadlock-rpc
```

## Pull Request Process

1. Rebase onto `main` before submitting
2. Ensure the project builds without warnings (`cargo build`)
3. Open a PR against `main` and describe what changed and why

## Commit Messages

Follow [Conventional Commits](https://conventionalcommits.org/):

```
<type>(<scope>): <description>
```

**Types:** `feature`, `fix`, `docs`, `refactor`, `chore`

**Examples:**
```
feature/presence: add spectator mode detection
fix/log: handle missing condebug file on first launch
docs/readme: update config table
```
