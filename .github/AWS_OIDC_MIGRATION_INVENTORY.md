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
| `rust.yml`           | 11         | push, pull_request, workflow_dispatch        | **dual-pass ✅** (OIDC trusted / static PRs) | 3 ✅ → 4 |
| `external.yml`       | 2          | push, pull_request                           | **dual-pass ✅** (OIDC trusted / static PRs) | 3 ✅ → 4 |
| `bridge.yml`         | 2          | push, pull_request, workflow_dispatch        | **dual-pass ✅** (PR #26927)                 | 3 ✅ → 4 |
| `release.yml`        | 1          | release, workflow_dispatch                   | sccache static; **S3 ops OIDC ✅** (release env) | 5 (S3 ✅) |

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
| `pull_request`     | any (same-repo **and** fork)         | `…:pull_request`               | **NO — by design**       | **blocked on A/B/C** (see roles doc)  |

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

### `release.yml` — 1 site — **Phase 5: S3 done (job split), sccache remains**

`release-build`, prefix `${{ matrix.os }}`. Triggers on `release: created`
(tag) + `workflow_dispatch`.

**Done — option (b), the job split.** `release-build` no longer configures AWS
credentials at all: the existing-archive download uses the bucket's public read
access, and all `sui-releases` writes moved to `upload-release-archives-to-s3`,
which declares `environment: release` and assumes the `release-s3` role via OIDC
(roles doc, Role 2). The `release` GitHub Environment exists with a custom
deployment branch policy (`main` + `devnet-v*`/`testnet-v*`/`mainnet-v*` tags).

**Remaining — the sccache call site.** Because of the split, `release-build`'s
OIDC sub is now a plain tag/branch ref (no environment), so giving sccache OIDC
needs role #1 to trust the release subs: `refs/tags/{devnet,testnet,mainnet}-v*`
for `release:` events plus `refs/heads/main` (already trusted) for dispatch.
Until that trust is added, sccache keeps static keys here — it must stay working
through Phase 7 or release builds lose the S3 cache.

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
   `refs/tags/{devnet,testnet,mainnet}-v*` (dispatch-from-main is already covered).
   Static keys remain on this one call site until then.

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
- **`pull_request`** → empty role → static-key fallback = **unchanged behavior**.
- **Phase 4** then resolves the PR path per A/B/C (RO role, no-cache, or fork split).
- **Phase 7** removes the static-key inputs once no event path needs them.

**Decision required to finish the mixed callers (Phase 4):** pick PR-cache option
**A** (all PRs → RO role), **B** (no PR cache), or **C** (same-repo RO, fork none)
— see `AWS_OIDC_ROLES.md` §"PR read-only cache". The Phase-3 dual-pass above is the
same regardless; A/B/C only changes the Phase-4 follow-up and when static keys die.
