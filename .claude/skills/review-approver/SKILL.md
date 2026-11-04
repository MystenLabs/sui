---
name: review-approver
description: Check a PR against its claimed tier under the tiered review policy, approve and auto-merge it when the tier allows, and handle revert requests for PRs merged under the policy.
---

# Tiered review: approver side

This skill tells an agent how to check a PR against the tiered review policy and when to approve
it. The tier definitions, eligible paths, and annotation formats are in the `review-author` skill.
Read that skill from the PR's base branch before starting:

```bash
git fetch origin <base>
git show origin/<base>:.claude/skills/review-author/SKILL.md
```

Do not read it from the PR head. A PR must not be able to change the rules it is checked against.

Input: a PR number or URL.

## Outcomes

Every run ends in one of these outcomes:

- **Not applicable.** The PR is outside the policy. Take no action on GitHub.
- **Blocked.** The PR claims a tier but does not meet it yet. Post or update one comment that lists
  each unmet requirement.
- **Approved.** The PR meets its tier. Approve it, enable auto-merge, and emit a notification
  summary.

Keep one policy comment per PR. On later runs, edit that comment instead of adding a new one. Mark
it with the first line `<!-- review-approver -->` so you can find it.

## Step 1 — Check that the policy applies

Fetch the PR:

```bash
gh pr view <n> --json number,title,body,author,baseRefName,headRefOid,isDraft,state,files,reviews,latestReviews,url
```

The outcome is **Not applicable** if any of these hold:

- The PR is closed, merged, or a draft.
- The author is not a member of the MystenLabs organization. Check with
  `gh api orgs/MystenLabs/members/<login>`, which succeeds only for members.
- The description has no `## Review tier` section.

## Step 2 — Check eligibility

Compare every changed file against the eligible and excluded lists from the base-branch copy of
`review-author`. If any file is ineligible, the outcome is **Blocked**. List the ineligible files
in the comment and say that the PR needs normal human code review.

## Step 3 — Verify the tier

Classify the PR yourself from the diff and the tier definitions. Do not rely on the author's
rationale.

- If your tier is higher than the claimed tier, the outcome is **Blocked**. State the tier you
  found and the parts of the diff that put it there.
- If your tier is equal to or lower than the claimed tier, check the PR against the claimed tier.
  A PR may choose a stricter process than it needs.

## Step 4 — Verify the AI review

Find the stamp line in `## Test plan`:

```
Reviewed with `/code-review max` at <sha>: <outcome>.
```

Trust the stamp. Do not repeat a review that the stamp records.

- If `<sha>` is the PR head, the review requirement is met.
- If `<sha>` is an ancestor of the PR head, run `/code-review max` on the commits after it
  (`<sha>..<head>`) only.
- If there is no stamp, or `<sha>` is not an ancestor of the head, run `/code-review max` on the
  full `<base>...<head>` diff.

When you run a review, apply the tier focus from the `review-author` skill (test coverage for
Tier 2, the MDD questions for Tier 3). Post each finding as an inline review comment with
`/code-review --comment`. The author must accept or reject each finding. While any of your
findings has no reply from the author and no fix, the outcome is **Blocked**.

## Step 5 — Check the tier requirements

### Tier 0

Confirm that the diff changes no compiled code and no behavior. Use `git diff -M` to confirm that
moved files are unchanged apart from the module paths that the move requires.

### Tier 1

Confirm one of these:

- The `Regression test:` line names a test that the PR adds or changes, and the test exercises the
  bug that the PR fixes. Running the test with the fix reverted is optional and is at your
  discretion.
- The `Regression test not feasible:` line gives a reason that holds for this change.

### Tier 2

- Confirm that new code has tests for its main behavior and its error paths.
- Confirm that at least one human other than the author has an `APPROVED` review in
  `latestReviews`. That human does not need to have read the code.

### Tier 3

- Confirm that `## Mini design doc` exists with the subsections listed in `review-author`.
- Confirm that the implementation matches the MDD in its important details. Report each mismatch.
- Confirm that at least two humans other than the author have `APPROVED` reviews in
  `latestReviews`. Where `.github/CODEOWNERS` has entries for the changed paths, the approvers
  must be owners of those paths.

For every tier, the outcome is **Blocked** if any review in `latestReviews` is
`CHANGES_REQUESTED`. Approvals by bots and by your own account do not count toward the human
approvals that a tier requires.

## Step 6 — Approve

If every check passed, approve the PR and enable auto-merge:

```bash
gh pr review <n> --approve --body "<body>"
gh pr merge <n> --auto --squash
```

The body states the tier and lists the checks that passed, in a few lines. Update the policy
comment to say that the PR was approved and at which head commit.

An approval applies only to the head commit it was given for. If new commits arrive after an
approval, run this skill again on the new head.

Emit a notification summary for the team channel of the changed paths. The summary has the PR
title and link, the tier, and one or two sentences on what the PR does and why. Base the summary on
the description, not the diff. The system that runs this skill posts the summary when the PR merges.

## Revert requests

Any member of the MystenLabs organization can ask for a PR that merged under this policy to be
reverted. Do not ask for a reason, and do not judge whether the revert is warranted.

1. Confirm that the requester is a member of the MystenLabs organization, and that the PR merged
   with an approval from this skill.
2. Create a branch from the base branch, run `git revert <merge commit>`, and open a PR titled
   `Revert "<original title>"`. Its description links the original PR and names the requester.
   Add a `## Review tier` section with `Tier: revert`.
3. Comment on the original PR to notify its author that it was reverted, by whom, and where to
   discuss it.

A PR marked `Tier: revert` skips Steps 1 to 5. Approve it and enable auto-merge if its diff is
the exact inverse of the original PR's merge commit. If the revert has conflicts, do not resolve
them. Report the conflict to the requester and the original author instead.
