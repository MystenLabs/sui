# Auditing developer documentation

Method notes that hold for any docs repository. Repo-specific facts — where the
source of truth lives, which trees are generated, what runs locally — belong in
that repo's own `docs/AGENTS.md`, which imports this file.

Written from audits that found a documented gRPC API that never existed, 44
mislabelled code fences, a protocol claim the network had retired, an example
whose error path could not run, 13 blank cards on a generated page, and four
redirects serving 404s.

## The rule everything else follows

**Check a page against the source it describes, not against itself.** A page can
be internally consistent, well-structured, correctly cross-linked, pass every
frontmatter and link check, and still document an API that has never existed.
That is the normal case for the defects worth finding.

So: for every claim you correct, name the file and revision that proves it. Pin
the revision — "the protos at `618b6c84`, which `Cargo.toml` pins, say X" beats
"the protos say X", and only the first survives review. If you cannot point at a
source line, you have an opinion, not a finding. Say so and leave the page alone.

## Defect classes worth hunting

- **APIs that were never real.** A page described an `EventService` with a
  `ListAuthenticatedEvents` RPC, request fields and all. No such service exists
  anywhere. Nothing internal to the page was wrong — it was coherent, complete,
  and fictional.
- **Renamed symbols with the old name still documented.** The service was real;
  the RPC had been renamed. Half-right pages are harder to spot than wholly
  wrong ones, and reviewers skim past the half that checks out.
- **Arity and signature drift.** A constructor documented with four parameters
  that takes three.
- **Retired claims.** Pages asserting a behaviour the system removed. These
  survive because every checker treats the *topic word* appearing on the page as
  a pass — a page saying "X bypasses consensus" scores a hit for "consensus".
- **Code fences with the wrong language.** Not cosmetic: example validators key
  on the fence tag, so a mislabelled block is silently skipped by compilation and
  typechecking. A wrong label buys an exemption from every code check, and a skip
  and a pass look identical in the report.
- **Examples whose error path cannot run.** A call that throws on failure,
  followed by a branch testing for a falsy return. Read the return type before
  trusting a snippet's control flow.
- **Generated data with empty fields.** A catalogue JSON shipped entries with
  `description: ""` and cards rendered blank. Data files backing a page need the
  same scrutiny as prose.
- **Redirect rot.** Destinations that 404, destinations that redirect again, and
  redirects whose source shadows a real page so that page is unreachable. Site
  builds validate links between pages and never validate a redirect.

## Running a sweep

Most of the damage in these audits came from sweeps that under-matched, then from
a PR claiming the sweep was complete.

1. **Key on markers that cannot appear in the wrong answer.** Keying on the
   obvious ones misses the tail — for Move in a `rust` fence, `public fun` /
   `has key` / `&mut TxContext` misses private functions, ability-less structs,
   and constant-only blocks. That exact pattern left eight blocks behind while
   the PR claimed zero.
2. **Print the matched marker for every hit.** A sweep whose output cannot be
   checked will be believed, which is worse than one that fails loudly.
3. **Check what the sweep leaves alone, too.** Over-matching is the same bug
   mirrored. One pass flagged pseudo-notation and genuine Rust as TypeScript;
   four of six "findings" were wrong.
4. **Re-run after fixing and paste the new count.** Never write "0 remain" from
   the memory of having fixed them.
5. **`grep -c` the exact string before calling a fix complete.** Pages repeat
   themselves — a bullet list followed by sections restating the same sentences
   verbatim. A review caught two untouched copies of a claim "fixed" once on the
   same page.
6. **Do not guess a rename from name similarity.** `sui-cli` became
   `sui-networks-gas`, but the closest name was `sui-client` at 0.82 — a
   different thing, offered as the fix. Above 0.85 only typos match; below it,
   report without a suggestion.

## Generated content

Every docs site generates part of its own tree. Editing a generated file is work
that disappears on the next build, and the fix belongs upstream.

**Run `git check-ignore -v <path>` before editing any page.** Do not read a
`.gitignore` to decide — rules are usually spread across several of them and the
command tells you which one matched. Silence means the file is yours, though not
that it is hand-written: some repos commit generated files. Check the site's
build scripts (`prebuild`, `prestart`, and anything named `generate-*` or
`fetch-*`) for what writes into the content tree.

A finding in generated content is still a finding. Route it to the upstream
repository and say so, rather than patching a file that regenerates.

## Verify against the live site

The repository does not tell you what visitors get.

```bash
curl -s -o /dev/null -w "%{http_code} %{redirect_url}\n" https://<host>/<path>
```

- Is a redirect destination real?
- Does a page render its data, or is the card blank in the HTML?
- Is a page reachable at its own URL, or does a redirect shadow it?

**A 404 from `curl` is not proof a page is unpublished.** Check `x-vercel-cache`
and `age` in the response headers: a cached 404 from one edge can be days old
and survives a query-string cache-buster, because Vercel keys static assets on
path alone. Cross-check the site's `sitemap.xml`, try the `.html` form of the
path, and ask someone on another network before reporting a page as missing.
This cost a wrong finding — a live skills page, with three genuinely blank
cards on it, was reported as not yet published.

Docusaurus trap: the site serves `/x`, not `/x/index`. `/operators/index` is a
404 even though `operators/index.mdx` exists, so a redirect pointing at an
`/index` path is broken however healthy the route map looks.

## Skills

`technical-documentation-writing` governs prose — no invented source code,
inline citations for technical claims, security review rules. Load it before
writing. `writing-voice.md` in this directory carries house voice rather than
mechanical style. Both are hand-maintained, not tracked by any registry, so
`npx skills` will never update them.

**A skill is not a source.** Skills are documentation, maintained alongside the
docs and drifting on the same schedule. Checking a page against a skill is still
checking docs against docs, and both can be wrong together — a `sui-cli` skill
went stale in exactly the way a page does. Use a skill to learn where the answer
lives and what correct looks like, then cite the code in the finding.

```bash
npx skills update -g -y     # updates only what ~/.agents/.skill-lock.json tracks
npx skills list             # what is installed, and from where
```

Skills installed outside the CLI are invisible to `update` and can sit frozen for
months while reporting no available updates. `npx skills add <owner>/<repo> --all
-g -y` re-registers them. Renames leave ghosts: `add` never removes, so compare
the directory listing against the lock's keys afterwards.

## Branch and PR conventions

- One defect class per branch, cut from the upstream default branch — never from
  whatever branch is checked out. Fetch first; these repos move several times a
  day. Check which remote is upstream: it is not always `origin`.
- **Never post comments on pull requests.** Report findings and replies in the
  conversation; the human decides what goes on the PR.
- No `Co-Authored-By` lines.
- Every number in a PR description comes from a command run just before writing
  it. Counts derived from a diff go stale the moment the base moves — after
  merging upstream in, recount and correct the description. One PR's "16 edits"
  became 10 when an overlapping PR landed.
- Do not rebase a branch that has been reviewed. Review comments anchor to blob
  SHAs and a force-push orphans them. Merge the upstream branch in instead.
- Stage paths by name. Never `git add -A` in a repo with generated or untracked
  working files.

## Writing the finding

Describe what is wrong and what proves it, in that order. The test plan is where
an audit is either credible or not.

- Name the file, the revision, and the symbol. "`stream.rs` calls
  `LedgerServiceClient::list_events`" is checkable; "the client uses a different
  API" is not.
- Record the negative searches. "Searched these three repositories for
  `EventService`, `ListAuthenticatedEvents`, and `GetObjectInclusionProof`. No
  hits." is what makes a claim of non-existence reviewable.
- State what you did not verify, and why. An honest gap costs nothing; an
  unstated one costs the reviewer's trust in everything else.
- Say what is still wrong and out of scope — a generated page, an upstream
  repository, a page the audit did not reach.

## Comparing before and after

To run a check against an older state, add a detached worktree and symlink
`node_modules` into it rather than stashing:

```bash
git worktree add -q --detach /tmp/prefix <sha>
ln -sfn "$PWD/docs/site/node_modules" /tmp/prefix/docs/site/node_modules
cd /tmp/prefix/docs/site && node scripts/audit-docs.mjs > /tmp/before.json
git worktree remove --force /tmp/prefix
```
