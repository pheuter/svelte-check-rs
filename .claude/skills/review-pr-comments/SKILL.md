---
name: review-pr-comments
description: Review PR comments, analyze validity, and create action items
---

Review pull request comments on the current branch's PR. Determine which to address in code and which to decline with a polite response.

## PR Context

### PR Info

!`gh pr view --json number,title,url,headRepository,headRepositoryOwner,author,state,baseRefName,headRefName --jq '{number, title, url, repo: .headRepository.name, owner: .headRepositoryOwner.login, author: .author.login, state, base: .baseRefName, head: .headRefName}'`

### Review Threads

!`read -r OWNER REPO NUMBER <<< "$(gh pr view --json number,headRepository,headRepositoryOwner --jq '"\(.headRepositoryOwner.login) \(.headRepository.name) \(.number)"')" && gh api graphql -f query="query { repository(owner: \"$OWNER\", name: \"$REPO\") { pullRequest(number: $NUMBER) { reviewThreads(first: 100) { nodes { id isResolved isOutdated path line comments(first: 50) { nodes { id databaseId body author { login } createdAt } } } } } } }" --jq ".data.repository.pullRequest.reviewThreads.nodes[] | {thread_id: .id, resolved: .isResolved, outdated: .isOutdated, path: .path, line: .line, comments: [.comments.nodes[] | {id: .databaseId, author: .author.login, body, created_at: .createdAt}]}"`

### Top-Level Comments

!`gh pr view --json comments --jq '.comments[] | {type: "top-level", id, author: .author.login, body, createdAt}'`

### Changed Files

!`gh pr diff --name-only`

## Workflow

### 1. Filter Comments

**Skip:**
- Resolved threads (`resolved: true`) — unless user asks to review them
- Deployment bot comments (vercel, etc.)
- Comments already replied to by the PR author
- Purely informational comments with no action requested

**Prioritize** human reviewers over bot comments. Note outdated threads but still analyze them.

### 2. Analyze Each Comment

For each open, actionable comment:
1. **Read the code** at the referenced file and line (with surrounding context)
2. **Understand the ask** — bug fix, style, architecture, performance, testing, docs, etc.
3. **Research if needed** — check for similar patterns, conventions (CLAUDE.md), related code

### 3. Decide: Address or Decline

**Address if** it identifies a bug, security issue, missing error handling, convention violation, or aligns with existing patterns.

**Decline if** it's over-engineering, out of scope, personal preference, premature optimization, or conflicts with project conventions.

**Borderline cases** — use AskUserQuestion to let the user decide.

### 4. Create Tasks

One task per actionable comment using TaskCreate:

- **Code changes**: Subject `"Fix: {description}"`, include comment quote, author, file:line, specific action, and `metadata: {comment_id, action: "code_change"}`
- **Responses**: Subject `"Reply: {topic}"`, include comment quote, reasoning, draft reply, and `metadata: {comment_id, action: "respond"}`

### 5. Summary

Show TaskList, then a summary table:

| Comment | Author | File | Decision | Task | Reason |
|---------|--------|------|----------|------|--------|
| Brief description | @user | path:line | Address/Decline | #id | Short reason |

Mention any resolved/skipped threads briefly below the table.

### 6. Get Direction

Use AskUserQuestion with these options:
- **Make code changes** — implement changes for "Address" items
- **Post responses** — reply on GitHub for "Decline" items
- **Both** — code changes and responses
- **Review only** — no action

### 7. Execute

Work through tasks: mark in_progress, execute, mark completed. Ask for clarification if anything is unclear.

## Response Tone

Write replies as a teammate — conversational, direct, concise. Acknowledge valid points even when declining. Explain the "why", reference specific code or constraints. No formulaic openers. Match the reviewer's formality.

## Reply Commands

```bash
# Reply to review thread comment
gh api --method POST repos/{owner}/{repo}/pulls/{number}/comments/{comment_id}/replies -f body="Your reply"

# Reply to top-level comment
gh api --method POST repos/{owner}/{repo}/issues/{number}/comments -f body="@username Your reply"
```
