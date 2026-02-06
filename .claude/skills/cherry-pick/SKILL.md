# Cherry-pick to Release Branch

Cherry-picks a commit from main to a release branch and creates a PR.

## Usage

```
/cherry-pick <commit-sha> <release-version>
```

Example: `/cherry-pick abc123 1.64`

## Arguments

$ARGUMENTS should contain two space-separated values:
1. The commit SHA to cherry-pick
2. The release version number (e.g., "1.64" or "1.65")

## Instructions

Parse the arguments to extract the commit SHA and release version. If arguments are missing or unclear, ask the user for clarification.

Execute the following steps:

### 1. Validate the commit exists
```bash
git show --oneline --no-patch <commit-sha>
```
Save the commit message for use in the PR title.

### 2. Fetch and checkout the release branch
The release branch naming convention is `releases/sui-v<version>.0-release`.
```bash
git fetch origin releases/sui-v<version>.0-release
git checkout releases/sui-v<version>.0-release
```

### 3. Cherry-pick the commit
```bash
git cherry-pick <commit-sha>
```

If there are conflicts:
- Inform the user about the conflicts
- Help them resolve the conflicts if they want
- After resolution, run `git cherry-pick --continue`

### 4. Create a branch for the PR
Use the naming convention `cherry-pick-<short-sha>-to-<version>`:
```bash
git checkout -b cherry-pick-<short-sha>-to-<version>
```

### 5. Push the branch
```bash
git push -u origin cherry-pick-<short-sha>-to-<version>
```

### 6. Create the PR
Create a PR targeting the release branch:
```bash
gh pr create --base releases/sui-v<version>.0-release \
  --title "[<version>] <original-commit-message>" \
  --body "## Summary
Cherry-pick of <original-pr-link-if-available> to the <version> release branch.

Original commit: <full-commit-sha>"
```

### 7. Return to main
```bash
git checkout main
```

### 8. Report the PR URL to the user
