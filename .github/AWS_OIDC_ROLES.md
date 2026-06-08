<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# AWS OIDC roles for GitHub Actions

Spec for the AWS IAM roles that let `MystenLabs/sui` workflows authenticate to
AWS via **GitHub OIDC** instead of long-lived `AWS_ACCESS_KEY_ID` /
`AWS_SECRET_ACCESS_KEY` secrets. This document is the source of truth for the
**AWS account owner** to provision the roles; the workflows reference the
resulting role ARNs via a `role-to-assume` input.

> **Why:** static keys are long-lived, broadly scoped, and must be rotated.
> OIDC mints short-lived, request-scoped credentials with a trust policy that
> pins exactly which repo/branch/tag/environment may assume the role.

## One-time: the GitHub OIDC identity provider

Create (once per account) the IAM OIDC provider:

- Provider URL: `https://token.actions.githubusercontent.com`
- Audience: `sts.amazonaws.com`

ARN used by the trust policies below:
`arn:aws:iam::<ACCOUNT_ID>:oidc-provider/token.actions.githubusercontent.com`

## Trust-policy rules (read this before writing any role)

1. **Always pin `:aud` to `sts.amazonaws.com`** (`StringEquals`).
2. **Always constrain `:sub`. Never use `repo:MystenLabs/sui:*`.** A wildcard
   `sub` would let **`pull_request`-triggered jobs assume the role** — GitHub's
   OIDC `sub` for a PR is `repo:MystenLabs/sui:pull_request`, so a fork or any PR
   could obtain AWS credentials. Pin to branch/tag/environment subjects.
3. **Preferred: a dedicated GitHub Environment** (e.g. `aws-sccache`) with
   environment protection rules restricting which branches/tags can deploy to
   it. The `sub` is then a single exact value
   `repo:MystenLabs/sui:environment:<name>`, and the branch restriction is
   enforced by the environment — the cleanest, least-error-prone option.
4. **Alternative: explicit `ref` subjects** via `StringLike` when not using an
   environment. Wildcards *within* a ref (e.g. `…:ref:refs/heads/releases/sui-*-release`)
   are fine; a bare `…:*` is not.
5. `pull_request` is intentionally excluded from the RW roles (it is the
   fork/untrusted path → no S3 write). See "PR read-only cache" below.

---

## Role 1 — `sui-github-actions-sccache`

**Used by:** the `setup-sccache` composite action (18 call sites). Read/write
to the shared compile cache, **only from trusted contexts**. Forks and PRs do
not assume this role (they fall back to a local cache).

### Trust policy

Preferred (environment `aws-sccache`):

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": { "Federated": "arn:aws:iam::<ACCOUNT_ID>:oidc-provider/token.actions.githubusercontent.com" },
    "Action": "sts:AssumeRoleWithWebIdentity",
    "Condition": {
      "StringEquals": {
        "token.actions.githubusercontent.com:aud": "sts.amazonaws.com",
        "token.actions.githubusercontent.com:sub": "repo:MystenLabs/sui:environment:aws-sccache"
      }
    }
  }]
}
```

Alternative (explicit refs — use if not adopting an environment):

```json
"Condition": {
  "StringEquals": { "token.actions.githubusercontent.com:aud": "sts.amazonaws.com" },
  "StringLike": {
    "token.actions.githubusercontent.com:sub": [
      "repo:MystenLabs/sui:ref:refs/heads/main",
      "repo:MystenLabs/sui:ref:refs/heads/devnet",
      "repo:MystenLabs/sui:ref:refs/heads/testnet",
      "repo:MystenLabs/sui:ref:refs/heads/mainnet",
      "repo:MystenLabs/sui:ref:refs/heads/releases/sui-*-release",
      "repo:MystenLabs/sui:ref:refs/tags/*-v*"
    ]
  }
}
```

(Scheduled and `workflow_dispatch` runs execute on a branch — usually `main` —
so they are covered by the branch subjects above.)

### Permission policy (least-privilege; bucket `mystenlabs-sccache`)

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "SccacheObjectRW",
      "Effect": "Allow",
      "Action": ["s3:GetObject", "s3:PutObject"],
      "Resource": "arn:aws:s3:::mystenlabs-sccache/*"
    },
    {
      "Sid": "SccacheListBucket",
      "Effect": "Allow",
      "Action": "s3:ListBucket",
      "Resource": "arn:aws:s3:::mystenlabs-sccache"
    }
  ]
}
```

### PR read-only cache (follow-up, not this PR)

Today same-repo PRs get **read/write** cache via the static key. The goal is to
demote PRs to **read-only** so a PR can't poison the shared cache, using a second
role `sui-github-actions-sccache-ro` (trust on `pull_request`, policy granting
only `s3:GetObject` + `s3:ListBucket`, **no `PutObject`**).

**Caveat — "fork = none" is not enforceable by the trust policy.** GitHub's OIDC
`sub` for *every* PR — same-repo and fork alike — is the same value
`repo:MystenLabs/sui:pull_request`. AWS cannot tell a fork PR from a same-repo PR
from that claim, so the trust alone cannot grant RO to same-repo PRs while
denying forks. Pick one:

- **(A) All PRs → read-only** (incl. forks): trust `pull_request` on the RO role.
  Simplest; a read-only cache is low-risk (no poisoning, no exfil beyond cache
  contents). Recommended unless cache contents are sensitive.
- **(B) No PR cache at all**: do not trust `pull_request`; same-repo PRs also
  lose the cache (slower CI), but the boundary is purely trust-policy-enforced.
- **(C) Same-repo → RO, fork → none, enforced at the workflow layer**: trust
  `pull_request`, but gate the input in the caller with
  `if: github.event.pull_request.head.repo.fork == false` so a fork job never
  passes `role-to-assume` (and never requests the token). Achieves the three-way
  split, but the fork exclusion lives in the *workflow*, not the trust policy —
  weaker (a future edit dropping the gate would re-admit forks). Document the
  dependency if chosen.

(Fork PRs run the **base** repo's workflow and get no repo secrets; whether they
can mint an `id-token` depends on the base workflow's `permissions`. Option C's
`if` gate is what makes the fork exclusion explicit and reviewable.)

---

## Role 2 — `sui-github-actions-release-s3`

**Used by:** `release.yml` — uploads/downloads release binaries to
`s3://sui-releases`. Runs on the `release: created` event (a release tag) and
`workflow_dispatch`. Keep this **separate** from the cache role (different
bucket, narrower trust).

### Trust policy — use a `release` environment (do NOT use tag subjects alone)

`release.yml` triggers on **both** `release: created` (a tag ref) **and**
`workflow_dispatch`. A manual dispatch runs with a **branch** subject
(`repo:MystenLabs/sui:ref:refs/heads/main`) *even when the `sui_tag` input names
a release tag* — so a tag-only `:sub` allowlist would reject the manual path and
break dispatch-triggered releases.

**Preferred — a protected `release` environment.** One exact subject covers both
event types, and the environment adds reviewer/branch gating (see "GitHub
Environment requirements" below). The release job declares `environment: release`.

```json
"Condition": {
  "StringEquals": {
    "token.actions.githubusercontent.com:aud": "sts.amazonaws.com",
    "token.actions.githubusercontent.com:sub": "repo:MystenLabs/sui:environment:release"
  }
}
```

If you must use explicit refs instead, allow the release tags **and** the exact
manual-dispatch branch — and gate that branch carefully (it is broader than a
tag and would also match any other job on `main`):

```json
"StringLike": {
  "token.actions.githubusercontent.com:sub": [
    "repo:MystenLabs/sui:ref:refs/tags/devnet-v*",
    "repo:MystenLabs/sui:ref:refs/tags/testnet-v*",
    "repo:MystenLabs/sui:ref:refs/tags/mainnet-v*",
    "repo:MystenLabs/sui:ref:refs/heads/main"
  ]
}
```

### Permission policy (bucket `sui-releases`)

Release archives are large (multi-hundred-MB `.tgz`), so `aws s3 cp` uses
**multipart** uploads — include the multipart actions (`s3:PutObject` authorizes
the create/upload/complete parts; `AbortMultipartUpload` and the list actions
cover cleanup of interrupted transfers):

```json
{
  "Version": "2012-10-17",
  "Statement": [
    { "Sid": "ReleaseObjectRW", "Effect": "Allow",
      "Action": ["s3:GetObject", "s3:PutObject", "s3:AbortMultipartUpload"],
      "Resource": "arn:aws:s3:::sui-releases/*" },
    { "Sid": "ReleaseListBucket", "Effect": "Allow",
      "Action": ["s3:ListBucket", "s3:ListBucketMultipartUploads"],
      "Resource": "arn:aws:s3:::sui-releases" }
  ]
}
```

> Note: `sccache-warmup.yml` writes a small summary to `s3://mystenlabs-sccache`.
> It can reuse Role 1 (it runs on `main`/scheduled), so no separate role is
> needed there.

---

## Role 3 — `sui-github-actions-kms-test`

**Used by:** the AWS KMS test in `turborepo.yml` (`kms:Sign`/`Verify` against a
**dedicated test key**). Scope the trust to the contexts `turborepo.yml` runs in.

- If it runs only on `main`/scheduled, use the same trusted-ref pattern as Role 1.
- If it must run on **PRs**, the minimal blast radius of a test-only KMS key may
  justify allowing `repo:MystenLabs/sui:pull_request` here — **owner's decision**.
  Unlike a cache bucket, a sign/verify-only test key carries no data-exfiltration
  or cache-poisoning risk. Document the choice explicitly.

### Permission policy (test key only)

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Sid": "KmsTestSignVerify",
    "Effect": "Allow",
    "Action": ["kms:Sign", "kms:Verify", "kms:GetPublicKey", "kms:DescribeKey"],
    "Resource": "arn:aws:kms:us-west-2:<ACCOUNT_ID>:key/<TEST_KEY_ID>"
  }]
}
```

The GCP KMS half of `turborepo.yml` migrates to **GCP Workload Identity
Federation** separately; its resource identifiers (`GOOGLE_PROJECT_ID`,
`GOOGLE_LOCATION`, `GOOGLE_KEYRING`, `GOOGLE_KEY_NAME`, `GOOGLE_KEY_NAME_VERSION`)
are not secrets and should move to `vars`.

---

## Caller wiring (workflow side)

Each job that assumes a role needs OIDC token permission and passes the ARN:

```yaml
permissions:
  id-token: write   # required to mint the OIDC token
  contents: read
...
    - uses: ./.github/actions/setup-sccache
      with:
        role-to-assume: arn:aws:iam::<ACCOUNT_ID>:role/sui-github-actions-sccache
        key-prefix: ${{ matrix.os }}
        # aws-access-key-id/secret omitted — OIDC takes priority
```

`setup-sccache` already supports this: it assumes the role when `role-to-assume`
is set, falls back to static keys when only those are set, and skips the S3
cache when neither is set (fork PRs). This lets callers migrate one at a time.
The action already sets `role-session-name` (`sui-sccache-<run_id>-<attempt>`).

## Hardening (apply per role / caller)

- **`allowed-account-ids`** — set it on `configure-aws-credentials` so a
  misconfigured role ARN can't silently assume into the wrong account. Pass the
  org's account id; for direct `configure-aws-credentials` steps (`release.yml`,
  `turborepo.yml`) add `allowed-account-ids: "<ACCOUNT_ID>"`. (If wired through
  `setup-sccache`, expose it as an input or set it in the caller.)
- **`role-session-name`** — include run/attempt identifiers
  (`…-${{ github.run_id }}-${{ github.run_attempt }}`) so every
  `AssumeRoleWithWebIdentity` in CloudTrail traces to a run. `setup-sccache` does
  this already; set it on direct callers too.
- **Least privilege + region pin** — keep `aws-region: us-west-2` and scope every
  policy to the exact bucket/key ARN (as above).

## GitHub Environment requirements (if using environments)

A `…:environment:<name>` subject is only as tight as the environment's rules. For
`aws-sccache` / `release`:

- Set **deployment branch/tag rules** to exactly the trusted refs (e.g. `main`,
  `releases/sui-*-release`, the release tag patterns). Without them, *any* branch
  could target the environment and obtain the role.
- Do **not** attach broad/unrelated secrets to the environment — it should grant
  the OIDC subject and nothing else.
- For `release`, add required reviewers if the release should be gated.

## Migration order

1. **`sccache-warmup.yml`** — safest first (scheduled/dispatch on `main`, RW).
2. **`nightly.yml`** — scheduled on `main`.
3. **PR CI** (`sui-ci-tests.yml` et al.) — introduce Role 1 (RW) for trusted
   refs and Role `…-sccache-ro` for `pull_request`, completing the
   protected=RW / PR=RO / fork=none split.
4. **`release.yml`** — Role 2, after the cache roles are proven.
