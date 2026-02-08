---
name: commit
description: Review current changes, commit, push, and optionally open a PR
argument-hint: "[pr or draft pr]"
---

Review the current git changes, create a commit, push to remote, and optionally open a pull request.

## User Request

$ARGUMENTS

## Current Git State

### Status

!`git status --short`

### Staged Changes

!`git diff --cached`

### Unstaged Changes

!`git diff`

### Recent Commits

!`git log --oneline -5`

### Current Branch

!`git branch --show-current`

### Default Branch

!`git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@'`

## Workflow

1. **Read full diff if truncated.** If changes above show "Output too large" with a file path, read that file first. Never review based on partial output.

2. **Review changes** for bugs, security issues (secrets, .env files), and incomplete work. If issues found, ask the user how to proceed.

3. **Prepare commit:**
   - If PR requested and on the default branch, create a descriptive branch first
   - Run any format/lint commands specified in project guidelines — fix any issues
   - Stage relevant changes (skip secrets/generated files)
   - Commit following project conventions
   - Push (use `-u origin <branch>` for new branches)

4. **If PR requested:** use `gh pr create` (add `--draft` if requested) with a clear title and description targeting the default branch. Follow PR conventions from CLAUDE.md.

If there are no changes to commit, let me know.
