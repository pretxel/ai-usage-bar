---
description: Create a Conventional Commits commit from staged (or all) changes
argument-hint: "[optional scope or note, e.g. 'backend pricing']"
allowed-tools: Bash(git status:*), Bash(git diff:*), Bash(git add:*), Bash(git log:*), Bash(git commit:*)
---

Create a [Conventional Commits](https://www.conventionalcommits.org/) commit for the current changes.

## Context

- Branch: !`git rev-parse --abbrev-ref HEAD`
- Status: !`git status --short`
- Staged diff: !`git diff --cached --stat`
- Unstaged diff: !`git diff --stat`
- Recent commits (for style): !`git log --oneline -10`

User note (optional): $ARGUMENTS

## Steps

1. **Decide what to stage.** If nothing is staged, review the unstaged/untracked changes and stage the files that belong together (`git add`). If the changes are unrelated, prefer staging only one logical group and say so. Never `git add -A` blindly — skip build output, secrets, and unrelated edits.

2. **Inspect the actual diff** with `git diff --cached` before writing the message. The message must describe what really changed, not what you assume.

3. **Write the message** in Conventional Commits format:

   ```
   <type>(<scope>): <subject>

   <body>

   <footer>
   ```

   - **type** — one of: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
   - **scope** — optional, lowercase, the area touched. For this repo good scopes are: `backend` (`lib.rs`), `frontend` (`App.tsx`), `pricing`, `tray`, `watcher`, `types`, `docs`, `build`. Use the user note above if given.
   - **subject** — imperative mood, lowercase, no trailing period, ≤ 50 chars ("add codex cache-read pricing", not "Added...").
   - **body** — optional, wrap at ~72 cols, explain the *why* not the *what*. Omit if the subject is self-explanatory.
   - **footer** — `BREAKING CHANGE: ...` for incompatible changes; reference issues like `Closes #12` if relevant.

4. **Match repo style.** Look at the recent commits above and stay consistent (this repo has no commits yet — establish a clean convention).

5. **Commit.** Use a single `git commit` with a heredoc so the body formats correctly:

   ```bash
   git commit -m "$(cat <<'EOF'
   feat(pricing): add codex cache-read pricing

   Codex transcripts report cached_input_tokens; map them to the
   cache_read bucket so spend lines up with the Claude path.
   EOF
   )"
   ```

6. **Confirm** by showing the resulting `git log --oneline -1`.

## Rules

- Do NOT push, create branches, or open PRs — commit only.
- Do NOT add `Co-Authored-By` or tool advertising footers unless the repo's recent commits already use them.
- If there are zero changes to commit, say so and stop.
- One concern per commit. If the diff mixes a fix and a refactor, commit the most coherent subset and note what's left.
