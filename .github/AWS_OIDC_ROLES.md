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

> **Caller migration status + per-call-site classification:** see
> [`AWS_OIDC_MIGRATION_INVENTORY.md`](./AWS_OIDC_MIGRATION_INVENTORY.md).

## One-time: the GitHub OIDC identity provider

Create (once per account) the IAM OIDC provider:

- Provider URL: `https://token.actions.githubusercontent.com`
- Audience: `sts.amazonaws.com`

ARN used by the trust policies below:
`arn:aws:iam::011083325127:oidc-provider/token.actions.githubusercontent.com`

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

> **Status: ALREADY PROVISIONED.** This role exists in account `011083325127` as
> **`arn:aws:iam::011083325127:role/sui-sccache-rw-github`** (created 2026-05-22),
> with the managed policy `sui-sccache-rw` (exactly the least-privilege policy
> below). Use this ARN as the `role-to-assume` value — **do not create a second
> role.** Its current trust matches the "explicit refs" variant below but is
> scoped to **only** the four protected branches:
> `repo:MystenLabs/sui:ref:refs/heads/{main,devnet,testnet,mainnet}` — `aud`
> pinned, no wildcard.
>
> **Gap to close if needed:** the live trust does **not** yet include
> `releases/sui-*-release` branches or `*-v*` tags. sccache callers that run on a
> release branch or tag therefore cannot assume it today; add those `sub` entries
> (per the "explicit refs" block) before flipping any release-branch/tag-triggered
> sccache caller.

### Trust policy

Preferred (environment `aws-sccache`):

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": { "Federated": "arn:aws:iam::011083325127:oidc-provider/token.actions.githubusercontent.com" },
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
- **(C) Same-repo → RO, fork → none, enforced at the workflow layer.** Trust
  `pull_request` on the RO role, then exclude forks in the workflow. **Gating only
  the `role-to-assume` input is NOT sufficient:** if the fork's job still carries
  `id-token: write` and the role trusts `pull_request`, a fork could mint the OIDC
  token and call `sts:AssumeRoleWithWebIdentity` directly from any step — the
  gated input doesn't stop it. The exclusion must remove the *capability*, not
  just the input. Two correct shapes:
    - **Skip the whole OIDC-capable job for forks**:
      `if: github.event.pull_request.head.repo.fork == false` on the job, so a fork
      PR never runs a job that has `id-token: write`; or
    - **Split the paths**: a same-repo-PR job that has `id-token: write` +
      `role-to-assume`, and a separate fork-PR job with **no** `id-token: write`
      and no role (local-cache fallback only).
  Either way the fork exclusion lives in the *workflow*, not the trust policy —
  weaker than (A)/(B) (a future edit that re-adds `id-token: write` to a
  fork-reachable job re-admits forks). Document the dependency if chosen.

(Fork PRs run the **base** repo's workflow and get no repo secrets, but they CAN
mint an `id-token` if the job grants `id-token: write` — which is exactly why the
fork exclusion must drop that permission, not just the role input.)

---

## Role 2 — `sui-github-actions-release-s3`

**Used by:** `release.yml` — uploads/downloads release binaries to
`s3://sui-releases`. Runs on the `release: created` event (a release tag) and
`workflow_dispatch`. Keep this **separate** from the cache role (different
bucket, narrower trust).

> **Status: ACTIVE.** Provisioned 2026-06-08 as
> **`arn:aws:iam::011083325127:role/sui-releases-rw-github`** (inline policy
> `sui-releases-rw` = exactly the least-privilege policy below). Trust =
> `repo:MystenLabs/sui:environment:release` (the environment variant), aud
> pinned, no wildcard. The `release` GitHub Environment exists with a custom
> deployment branch policy (`main` branch + `devnet-v*`/`testnet-v*`/`mainnet-v*`
> tags), and `release.yml`'s `upload-release-archives-to-s3` job declares
> `environment: release` and assumes this role.

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

### Credential sequencing — `release.yml` touches BOTH buckets

`release.yml` builds with sccache (`mystenlabs-sccache`, Role 1) **and** uploads
artifacts to `sui-releases` (Role 2). `configure-aws-credentials` (and therefore
`setup-sccache`) **overwrites the AWS credential env vars** each time it runs —
whichever role was configured last wins within a job.

Resolved with a **two-job split**: `release-build` no longer holds any
`sui-releases` credentials — its only remaining AWS credentials are the static
sccache keys `setup-sccache` configures for the cache bucket (pending Role 1
trust for release-event subs), and the existing-archive download uses the
bucket's public read access. The `upload-release-archives-to-s3` job —
`environment: release`, Role 2 — receives the archives as workflow artifacts
and performs every `sui-releases` write. The two roles never coexist in one
job, so there is no credential sequencing to manage.

---

## Role 3 — `sui-github-actions-kms-test`

> **Status: NOT created — BLOCKED (2026-06-08).** Two blockers: (1) the AWS KMS
> test in `turborepo.yml` is **disabled** (`E2E_AWS_KMS_TEST_ENABLE: "false"`), so
> there's nothing to migrate yet; (2) the signing key is referenced only via the
> secret `AWS_KMS_TEST_KMS_KEY_ID` and has **no discoverable alias** in us-west-2,
> so the permission policy below can't be scoped to a real key ARN without
> guessing. **Unblock:** supply the test key ARN (from that secret) and confirm
> the test is being re-enabled; then create the role with the
> `repo:MystenLabs/sui:environment:sui-typescript-aws-kms-test-env` subject (that
> environment already exists) scoped to the one key. Good news: the env-subject is
> available, so the trust shape is settled — only the key ARN + re-enable remain.

**Used by:** the AWS KMS test in `turborepo.yml` (`kms:Sign`/`Verify` against a
**dedicated test key**, job-scoped to environment `sui-typescript-aws-kms-test-env`).
Scope the trust to that environment subject.

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
    "Resource": "arn:aws:kms:us-west-2:011083325127:key/<TEST_KEY_ID>"
  }]
}
```

The GCP KMS half of `turborepo.yml` migrates to **GCP Workload Identity
Federation** separately; its resource identifiers (`GOOGLE_PROJECT_ID`,
`GOOGLE_LOCATION`, `GOOGLE_KEYRING`, `GOOGLE_KEY_NAME`, `GOOGLE_KEY_NAME_VERSION`)
are not secrets and should move to `vars`.

---

## Caller wiring (workflow side)

Set `id-token: write` at **job level** (not workflow level) — only the
AWS-touching job should be able to mint an OIDC token:

```yaml
jobs:
  build:
    permissions:
      id-token: write   # required to mint the OIDC token — job-scoped
      contents: read
    steps:
    - uses: ./.github/actions/setup-sccache
      with:
        # Pass the role only on trusted refs, so a workflow_dispatch from a
        # non-main ref (whose sub the role does NOT trust) falls back to a local
        # cache instead of failing the AssumeRole. For schedule/push this is
        # always main.
        role-to-assume: ${{ github.ref == 'refs/heads/main' && 'arn:aws:iam::011083325127:role/sui-sccache-rw-github' || '' }}
        key-prefix: ${{ matrix.os }}
        # aws-access-key-id/secret omitted — OIDC takes priority
```

`setup-sccache` already supports this: it assumes the role when `role-to-assume`
is set, falls back to static keys when only those are set, and skips the S3
cache when neither is set (fork PRs / off-ref dispatch). This lets callers
migrate one at a time. The action sets `role-session-name`
(`sui-sccache-<run_id>-<attempt>`) and `allowed-account-ids: 011083325127` on
both configure-credentials paths.

## Hardening (apply per role / caller)

- **`allowed-account-ids`** — set it on `configure-aws-credentials` so a
  misconfigured role ARN can't silently assume into the wrong account.
  `setup-sccache` already hardcodes `allowed-account-ids: '011083325127'` on both
  its OIDC and static paths. For direct `configure-aws-credentials` steps
  (`release.yml`, `turborepo.yml`) add `allowed-account-ids: "011083325127"`
  explicitly.
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
