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

### Recent Commits (for message style reference)

!`git log --oneline -10`

### Current Branch

!`git branch --show-current`

### Default Branch

!`git remote show origin | grep 'HEAD branch' | cut -d' ' -f5`

## Steps

1. **IMPORTANT: Ensure you have the complete diff.** If the staged or unstaged changes above are truncated (showing "Output too large" with a saved file path), you MUST use the Read tool to read the full diff file before proceeding. Never review or commit based on partial/truncated output.

2. Review ALL the changes for:

   - Code quality issues
   - Potential bugs or errors
   - Security concerns (no secrets, credentials, .env files)
   - Missing or incomplete implementations
   - Consider running `cargo test` to verify changes don't break existing functionality

3. If issues are found, use **AskUserQuestion** to ask how to proceed:

   - Present the issues clearly
   - Offer options: fix the issues, proceed anyway, or abort

4. If changes look good (or user approves):

   - If user requested a PR and we're on main/master, create a new branch first with a descriptive name based on the changes
   - **IMPORTANT: Run `cargo fmt` to format the entire workspace before staging.** This must be done every time, even if you think files are already formatted.
   - Run `cargo clippy --all-targets -- -D warnings` to check for lint issues. Fix any warnings before committing.
   - Stage all relevant changes (avoid staging secrets or generated files)
   - Create a commit with a conventional commit message based on the changes
   - Push to the current branch (use `-u origin <branch>` if new branch)

5. If user requested a PR:
   - Use `gh pr create` to open a pull request
   - If user requested a draft PR, use `--draft` flag
   - Generate a clear title and description based on the changes
   - Target the default branch (usually `main`)

If there are no changes to commit, let me know.

## Commit Message Guidelines

- Use conventional commits format: `type(scope): description`
- Types: `feat`, `fix`, `docs`, `refactor`, `test`, `perf`, `build`, `ci`, `chore`
- Scopes are optional; use semantic scopes as needed (examples: `parser`, `transformer`, `diagnostics`, `a11y`, `css`, `cli`, `tsgo`, `bun`, `compiler`)
- Keep the first line under 72 characters
- Reference the "why" not just the "what"
- Match the style of recent commits shown above

## PR Guidelines

- PR title should match the commit message style (conventional commits)
- PR description should include:
  - Brief summary of changes using clear headings (e.g., **Summary**, **Details**, **Testing**)
  - Any relevant context or motivation
  - Testing notes if applicable
- Use `--draft` when work is in progress or needs review before finalizing
- When merging PRs, use **squash** merges and delete the branch (local + remote)

## Tool Usage

- **AskUserQuestion**: Use this tool whenever clarification is needed:
  - When issues are found during review (step 3)
  - When it's unclear which files should be staged
  - When the commit scope is ambiguous (one commit vs multiple)
  - When PR details need user input (title, description, draft vs ready)
- **Task tools**: For complex scenarios involving multiple commits or extensive changes, use TaskCreate/TaskUpdate/TaskList to track progress
