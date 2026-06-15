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
trust (aud pinned, no wildcard) = these subjects:
`repo:MystenLabs/sui:ref:refs/heads/{main,devnet,testnet,mainnet}` **plus**
`repo:MystenLabs/sui:ref:refs/heads/releases/sui-*-release` (added 2026-06-08).
Keep any new `SCCACHE_ROLE` gate in sync with this exact set.

## Status

| Workflow             | Call sites | Triggers                                     | Auth today                                   | Phase    |
| -------------------- | ---------- | -------------------------------------------- | -------------------------------------------- | -------- |
| `sccache-warmup.yml` | 2          | schedule, workflow_dispatch                  | **OIDC ✅**                                  | 1 ✅     |
| `nightly.yml`        | 1          | schedule, workflow_dispatch (disabled)       | **OIDC ✅**                                  | 2 ✅     |
| `rust.yml`           | 11         | push, pull_request, workflow_dispatch        | **OIDC ✅** (RW trusted / RO same-repo PR / local fork) | 3 ✅ 4 ✅ |
| `external.yml`       | 2          | push, pull_request                           | **OIDC ✅** (RW trusted / RO same-repo PR / local fork) | 3 ✅ 4 ✅ |
| `bridge.yml`         | 2          | push, pull_request, workflow_dispatch        | **OIDC ✅** (RW trusted / RO same-repo PR / local fork) | 3 ✅ 4 ✅ |
| `release.yml`        | 1          | release, workflow_dispatch                   | **OIDC ✅** (sccache RW + S3 ops release env) | 5 ✅     |

## How each event/ref classifies

The decision is the OIDC `sub` of the run, which depends on the event + ref:

| Event              | Ref                                  | `sub`                          | Assume RW role #1 today? | Plan                                  |
| ------------------ | ------------------------------------ | ------------------------------ | ------------------------ | ------------------------------------- |
| `push`             | `main`/`devnet`/`testnet`/`mainnet`  | `…:ref:refs/heads/<b>`         | **YES**                  | OIDC RW (trusted)                     |
| `push`             | `releases/sui-*-release`             | `…:ref:refs/heads/releases/…`  | **YES** (added 2026-06-08) | OIDC RW (trusted)                   |
| `push`             | `extensions` (external.yml only)     | `…:ref:refs/heads/extensions`  | **NO — excluded**        | branch is 404; local-cache fallback   |
| `workflow_dispatch`| `main`, **no** `sui_repo_ref`        | `…:ref:refs/heads/main`        | **YES**                  | OIDC RW                               |
| `workflow_dispatch`| `main` + `sui_repo_ref=<feature>`    | `…:ref:refs/heads/main`        | sub matches, but **gate OFF** | builds the override ref (untrusted code) → must NOT hold RW; gate requires `sui_repo_ref==''` → static/local |
| `workflow_dispatch`| feature branch                       | `…:ref:refs/heads/<feat>`      | **NO**                   | local-cache fallback (expected)       |
| `pull_request` (same-repo) | PR merge ref                 | `…:pull_request`               | **NO** (RO, not RW)      | **RO role ✅** (option A)              |
| `pull_request` (fork)      | PR merge ref                 | `…:pull_request`               | **NO**                   | empty → local cache (fork `id-token` capped to none; gate skips to avoid a hard-fail — fail-safe, not a security boundary) |

> **Checkout-ref caveat (rust.yml / bridge.yml).** The OIDC `sub` (and thus role
> assumability) is decided by `github.ref` — but these workflows check out
> `${{ github.event.inputs.sui_repo_ref || github.ref }}`. So a `workflow_dispatch`
> from `main` with `sui_repo_ref` set to a feature ref has a **trusted `sub`** while
> **building untrusted code**. The `SCCACHE_ROLE` gate must therefore also require
> `sui_repo_ref` to be empty, or that build would populate the RW cache from
> arbitrary code. `external.yml` has no such input, so it is unaffected.

## Per-call-site map

### `rust.yml` — 11 sites (`push`[main,devnet,testnet,mainnet,releases/sui-*-release] + `pull_request` + `workflow_dispatch`)

`test` (L97), `test-tidehunter` (L146), `test-extra` (L211), `benchmark-smoke`
(L276), `windows-build` (L334), `windows-cli-tests` (L361), `simtest` (L386),
`simtest-mainnet` (L422), `move-test` (L458), `clippy` (L554), `sui-excution-cut`
(L635). Prefixes: `ubuntu-ghcloud` (×9), `windows-ghcloud` (×2).

### `external.yml` — 2 sites (`push`[main,extensions,devnet] + `pull_request`)

`external-crates-test` (L66), `clippy` (L107). Prefix `ubuntu-ghcloud`.
Note `extensions` push is **not** trusted (branch is 404 → excluded) → local cache.

### `bridge.yml` — 2 sites (`push`[main,devnet,testnet,mainnet,releases/sui-*-release] + `pull_request` + `workflow_dispatch`)

`clippy` (L85), `test` (L111). Prefix `ubuntu-ghcloud`.

### `release.yml` — 1 site — **Phase 5: S3 + sccache both OIDC ✅**

`release-build`, prefix `${{ matrix.os }}`. Triggers on `release: created`
(tag) + `workflow_dispatch`.

**S3 ops — option (b) job split.** `release-build` holds no `sui-releases`
credentials: the existing-archive download uses the bucket's public read access,
and all `sui-releases` writes moved to `upload-release-archives-to-s3`, which
declares `environment: release` and assumes the `release-s3` role via OIDC (roles
doc, Role 2). The `release` GitHub Environment exists with a custom deployment
branch policy (`main` + `devnet-v*`/`testnet-v*`/`mainnet-v*` tags).

**sccache — flipped to OIDC RW.** Role 1 now trusts the release tag subs
`refs/tags/{devnet,testnet,mainnet}-v*` (for `release:` events) in addition to
`refs/heads/main` (for dispatch). `release-build` declares
`permissions: {contents: write, id-token: write}` (contents:write is for the GH
release attach, not new — it had it via the default token) and passes the RW ARN
to `setup-sccache`. The role is unconditional here: there is no `pull_request`
trigger, and both event paths run under trusted subs.

**Dispatch hazard — closed.** A `workflow_dispatch` runs with sub
`refs/heads/main` (trusted) while checking out `inputs.sui_tag`. The two guards
that make this sound landed in PR #26953: the checkout uses `refs/tags/${sui_tag}`
explicitly, and the tag-name validation rejects anything not matching
`{devnet,testnet,mainnet}-v*` before `setup-sccache` runs — so dispatch only ever
builds published release tags. Static-key inputs are retained until Phase 7.

## Trust-policy gaps — status

Whether role #1 trusts each ref a trusted-ref flip needs (else it silently drops
to local cache):

1. **`releases/sui-*-release` branches** — **ADDED 2026-06-08** (StringLike; 4 exact
   branches preserved, `aud` pinned). Covers `rust.yml` + `bridge.yml` protected
   release-branch push CI.
2. **`extensions` branch** — **excluded.** The branch returns 404 (not active), so
   trusting it now is unused attack surface. Add only if it is confirmed active again;
   until then `external.yml` `extensions` pushes fall back to local cache.
3. **`release.yml` sccache** — **now a tag-trust gap (post job-split).** With the
   S3 ops moved to their own `environment: release` job, `release-build`'s sub is a
   plain tag/branch ref. Giving its sccache step OIDC requires role #1 to trust
   `refs/tags/{devnet,testnet,mainnet}-v*`. The dispatch path's `main` sub is
   already trusted but carries the chosen-ref hazard — see the `release.yml`
   section for the two mandatory guards before flipping. Static keys remain on
   this one call site until then.

## Recommended Phase-3 mechanic for mixed-trigger callers

`setup-sccache` prefers `role-to-assume` when set and falls back to static keys
when only those are set. That lets a single call site be flipped **without
changing PR behavior**. Compute the role once at the **workflow level** (a
workflow-level `env` value may reference the `github` context — actionlint-confirmed)
and reference it from each sccache job. As implemented + validated in `bridge.yml`
(PR #26927):

```yaml
env:
  # OIDC RW role only when BOTH: (1) the ref/sub is trusted, AND (2) we are not a
  # workflow_dispatch building an override ref (sui_repo_ref). Keep the ref set in
  # sync with the role trust. Drop the sui_repo_ref clause for workflows lacking
  # that input (e.g. external.yml).
  SCCACHE_ROLE: ${{ (github.event_name != 'workflow_dispatch' || github.event.inputs.sui_repo_ref == '') && (contains(fromJSON('["refs/heads/main","refs/heads/devnet","refs/heads/testnet","refs/heads/mainnet"]'), github.ref) || (startsWith(github.ref, 'refs/heads/releases/sui-') && endsWith(github.ref, '-release'))) && 'arn:aws:iam::011083325127:role/sui-sccache-rw-github' || '' }}

jobs:
  <sccache-job>:
    permissions:
      contents: read
      id-token: write # only the sccache jobs, not the whole workflow
    steps:
      - uses: ./.github/actions/setup-sccache
        with:
          role-to-assume: ${{ env.SCCACHE_ROLE }} # OIDC on trusted refs; empty elsewhere
          # retained so same-repo PRs keep today's RW cache until Phase 4 decides A/B/C
          aws-access-key-id: ${{ secrets.AWS_ACCESS_KEY_ID }}
          aws-secret-access-key: ${{ secrets.AWS_SECRET_ACCESS_KEY }}
          key-prefix: ...
```

The `sui_repo_ref` guard is **conservative** for jobs that ignore that input
(e.g. `bridge.yml`'s `test` checks out the default ref) — they fall back to
static/local on an override dispatch rather than OIDC. That is the safe direction.

- **Trusted push / dispatch-from-main** → OIDC RW (no static keys used).
- **`pull_request`** → empty role → static-key fallback (until Phase 4 lands).
- **Phase 4** then resolves the PR path: **option A — all PRs → RO role.**
- **Phase 7** removes the static-key inputs once no event path needs them.

**Phase 4 decision: option A (2026-06-15) — IMPLEMENTED.** The security decision:
PRs get **read-only** sccache via the RO role (`sui-sccache-ro-github`, trust
`repo:MystenLabs/sui:pull_request`); no PR gets write access, so poisoning stays
blocked. Chosen over C because the cache holds only public-source build artifacts
(no identified sensitivity) and C's "fork = none" cannot be done in IAM — every PR
presents the same sub `repo:MystenLabs/sui:pull_request`.

**Implementation (the fork nuance).** GitHub caps fork-PR `id-token` to `none`, so
a fork cannot mint the OIDC token, and `setup-sccache`'s OIDC step has no
`continue-on-error` — setting `role-to-assume` on a fork would hard-fail the job.
So each `SCCACHE_ROLE` gate resolves the RO ARN only for **same-repo** PRs
(`github.event.pull_request.head.repo.fork == false`); **fork** PRs get empty →
local cache, exactly as before. This fork skip is a **fail-safe accommodation, not
a security boundary**: the RO role still trusts `pull_request`, so if GitHub
settings ever let forks mint `id-token`, a fork assuming the RO role is still
acceptable under Decision A — fork-local must NOT be relied on as a durable
guarantee. Static-key inputs are retained on all call sites until Phase 7 (the
RO/RW OIDC paths are verified first, then the static fallback is removed).
