<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# AWS OIDC migration — `setup-sccache` caller inventory

Companion to [`AWS_OIDC_ROLES.md`](./AWS_OIDC_ROLES.md). Classifies every
`setup-sccache` call site so the static-key → OIDC migration can be sequenced per
**call site and per event/ref**, not per file. Mixed-trigger workflows
(`push` + `pull_request`) are split across both: a single job is "trusted" on a
protected-branch push and "untrusted" on a PR.

The RW cache role is `arn:aws:iam::011083325127:role/sui-sccache-rw-github`. Its
trust today = **only** these subjects (aud pinned, no wildcard):
`repo:MystenLabs/sui:ref:refs/heads/{main,devnet,testnet,mainnet}`.

## Status

| Workflow             | Call sites | Triggers                                     | Auth today  | Phase |
| -------------------- | ---------- | -------------------------------------------- | ----------- | ----- |
| `sccache-warmup.yml` | 2          | schedule, workflow_dispatch                  | **OIDC ✅** | 1 ✅  |
| `nightly.yml`        | 1          | schedule, workflow_dispatch (disabled)       | **OIDC ✅** | 2 ✅  |
| `rust.yml`           | 11         | push, pull_request, workflow_dispatch        | static keys | 3/4   |
| `external.yml`       | 2          | push, pull_request                           | static keys | 3/4   |
| `bridge.yml`         | 2          | push, pull_request, workflow_dispatch        | static keys | 3/4   |
| `release.yml`        | 1          | release, workflow_dispatch                   | static keys | 5     |

## How each event/ref classifies

The decision is the OIDC `sub` of the run, which depends on the event + ref:

| Event              | Ref                                  | `sub`                          | Assume RW role #1 today? | Plan                                  |
| ------------------ | ------------------------------------ | ------------------------------ | ------------------------ | ------------------------------------- |
| `push`             | `main`/`devnet`/`testnet`/`mainnet`  | `…:ref:refs/heads/<b>`         | **YES**                  | OIDC RW (trusted)                     |
| `push`             | `releases/sui-*-release`             | `…:ref:refs/heads/releases/…`  | **NO — trust gap**       | add ref to role trust → OIDC RW       |
| `push`             | `extensions` (external.yml only)     | `…:ref:refs/heads/extensions`  | **NO — trust gap**       | add ref to role trust, or local cache |
| `workflow_dispatch`| `main`                               | `…:ref:refs/heads/main`        | **YES**                  | OIDC RW                               |
| `workflow_dispatch`| feature branch                       | `…:ref:refs/heads/<feat>`      | **NO**                   | local-cache fallback (expected)       |
| `pull_request`     | any (same-repo **and** fork)         | `…:pull_request`               | **NO — by design**       | **blocked on A/B/C** (see roles doc)  |

## Per-call-site map

### `rust.yml` — 11 sites (`push`[main,devnet,testnet,mainnet,releases/sui-*-release] + `pull_request` + `workflow_dispatch`)

`test` (L97), `test-tidehunter` (L146), `test-extra` (L211), `benchmark-smoke`
(L276), `windows-build` (L334), `windows-cli-tests` (L361), `simtest` (L386),
`simtest-mainnet` (L422), `move-test` (L458), `clippy` (L554), `sui-excution-cut`
(L635). Prefixes: `ubuntu-ghcloud` (×9), `windows-ghcloud` (×2).

### `external.yml` — 2 sites (`push`[main,extensions,devnet] + `pull_request`)

`external-crates-test` (L66), `clippy` (L107). Prefix `ubuntu-ghcloud`.
Note `extensions` push is a trust gap.

### `bridge.yml` — 2 sites (`push`[main,devnet,testnet,mainnet,releases/sui-*-release] + `pull_request` + `workflow_dispatch`)

`clippy` (L85), `test` (L111). Prefix `ubuntu-ghcloud`.

### `release.yml` — 1 site — **Phase 5, special**

`release-build` (L155), prefix `${{ matrix.os }}`. Triggers on `release: created`
(tag) + `workflow_dispatch`. **Two reasons it is not part of the Phase-3 batch:**
1. It also writes to `s3://sui-releases` via the **separate** `release-s3` role in
   the **same job**, and `configure-aws-credentials` overwrites cred env vars — the
   job must re-assume the right role before each S3 op (see roles doc §"Credential
   sequencing"). The `release` GitHub Environment for that role now exists.
2. Its sccache run is on a **release tag** (`*-v*`), which is **not** in role #1's
   trust — a separate gap from the release-branch one.

## Trust-policy gaps blocking the "trusted" path

Before a trusted-ref flip can actually use the cache (rather than silently
dropping to local) on these refs, add the subject(s) to role #1's trust:

1. **`releases/sui-*-release` branches** — needed by `rust.yml`, `bridge.yml` push CI.
2. **`extensions` branch** — needed by `external.yml` push CI (decide if worth it).
3. **`*-v*` tags** — needed by `release.yml` sccache (Phase 5).

Until added, those refs fall back to local cache (no breakage, just no shared cache).

## Recommended Phase-3 mechanic for mixed-trigger callers

`setup-sccache` prefers `role-to-assume` when set and falls back to static keys
when only those are set. That lets a single call site be flipped **without
changing PR behavior**, by passing **both**:

```yaml
- uses: ./.github/actions/setup-sccache
  with:
    # trusted refs → OIDC RW; empty elsewhere
    role-to-assume: ${{ contains(fromJSON('["refs/heads/main","refs/heads/devnet","refs/heads/testnet","refs/heads/mainnet"]'), github.ref) && 'arn:aws:iam::011083325127:role/sui-sccache-rw-github' || '' }}
    # retained so same-repo PRs keep today's RW cache until Phase 4 decides A/B/C
    aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
    aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
    key-prefix: ...
```

- **Trusted push / dispatch-from-main** → OIDC RW (no static keys used).
- **`pull_request`** → empty role → static-key fallback = **unchanged behavior**.
- **Phase 4** then resolves the PR path per A/B/C (RO role, no-cache, or fork split).
- **Phase 7** removes the static-key inputs once no event path needs them.

**Decision required to finish the mixed callers (Phase 4):** pick PR-cache option
**A** (all PRs → RO role), **B** (no PR cache), or **C** (same-repo RO, fork none)
— see `AWS_OIDC_ROLES.md` §"PR read-only cache". The Phase-3 dual-pass above is the
same regardless; A/B/C only changes the Phase-4 follow-up and when static keys die.
