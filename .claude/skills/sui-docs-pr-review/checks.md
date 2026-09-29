# Check catalogue

What to check, what proves it, and what each check cannot see.

## Do not invent rules from silence

The style guide is prescriptive where it speaks and **permissive where it is
silent**. A list of where a thing *is* called for is not a whitelist banning it
everywhere else. Before flagging anything as a violation, you need one of:

- the guide saying not to do it, or
- the guide saying to do the opposite.

"The guide lists three uses for bold and this is not one of them" is not a
finding. Neither is "the guide does not mention tables."

When the guide is silent, **the repo's own convention decides**, so go count:

```bash
grep -rl '<the pattern>' docs/content | wc -l      # how common is it already?
```

A pattern used on dozens of existing pages is the house style, whatever you
infer from an omission.

Known-correct, never flag:

- **Bold table header rows** (`| **Column** |`). Used on ~50 pages. Correct.
- **First person in frontmatter `questions:`.** Used on ~370 pages. Correct.
  (First person in an H2 body heading is still worth raising.)

This cost a real review: three published comments told an author to de-bold
table headers that were correct, on the strength of a whitelist that was never
a whitelist.

## Source of truth

Never verify a claim against another doc page. A skill is not a source either —
skills drift on the same schedule as the docs they describe.

| Claim about | Verify against |
|---|---|
| Move framework functions, structs, abilities, error constants | `crates/sui-framework/packages/**/sources/*.move` |
| Move stdlib (`std::*`) | `crates/sui-framework/packages/move-stdlib/sources/*.move` |
| TypeScript SDK behaviour | `~/ts-sdks`, and `git fetch` first — a local checkout weeks old will report a real API as nonexistent |
| gRPC services, RPCs, messages | `MystenLabs/sui-apis` |
| Protocol behaviour | root `CLAUDE.md`, `docs/content/develop/transactions/transaction-lifecycle.mdx` |
| CLI flags and output | `crates/sui/src/…`, and run the command |
| What a page renders as | `https://docs.sui.io/…` |

Pin the revision in the finding. "`origin/main` at `df494cdb` (2026-09-18)"
survives review; "the SDK says" does not.

### Verifying a Move example

Read the real signature and compare parameter order, arity, and generics:

```bash
grep -n "public fun <name>\|entry fun <name>\|public struct <Name>" \
  crates/sui-framework/packages/sui-framework/sources/<module>.move
```

Then check the example's **imports** against every symbol it calls. A module
that calls `internal::permit<APP>()` without `use std::internal;` does not
compile, and nothing in the prose will tell you.

### Verifying a TypeScript example

```bash
cd ~/ts-sdks && git fetch origin --quiet
git grep -n "<symbol>" origin/main -- packages/sui/src | head
git show origin/main:packages/sui/src/transactions/<File>.ts > /tmp/f.ts
```

Check the option-object type, not just that the method exists. Then apply
`sui-ts-docs-patterns/patterns.md` — the recurring hits are Rule 1 (client
constructor never shown), Rule 4 (`new Ed25519Keypair()` for a signer), Rule 5
and 6 (`result.Transaction` read with no `waitForTransaction` and no `$kind`
narrowing, which throws a TypeError on the failure path instead of surfacing the
abort).

Also check for undeclared bindings across blocks, and unguarded `.find()`
dereferences. A fence carrying `title='file.ts'` claims to be a standalone file
and should be runnable as one.

## Defect classes, in rough order of value

1. **APIs that were never real.** Coherent, complete, fictional. Search all
   plausible repos and record the negative searches.
2. **Renamed symbols, old name documented.** Half-right pages survive skimming.
3. **Arity and signature drift.**
4. **Retired claims.** Survive because term-presence checkers score the topic
   word as a pass.
5. **Examples that cannot compile or run.** Missing imports, wrong control flow
   on a throwing call, undeclared bindings.
6. **Mislabelled code fences.** A wrong language tag exempts the block from
   every code check, and a skip looks like a pass in the report.
7. **Generated data with empty fields.**
8. **Redirect rot.** Site builds never validate redirects.

## The mechanical sweep

`scripts/sweep.sh` covers the style-guide rules that grep can decide. It prints
the matched marker for every hit so the output is checkable.

What it finds: em dashes, Latin abbreviations, banned words (`may`, `via`,
causal `since`, `simple`, `dApp`, leading `Note`), ampersands, exclamation
marks, JSON-RPC, first person, spelled-out numbers, common idioms, "has to",
passive-voice markers, bold table headers, heading case, fence languages,
admonition count and type, manual footer sections, quotation marks, italics,
and character count.

What it cannot find, so you must:

- **The truth of any claim.** The whole point of steps 1 and 3.
- **The language on a code fence.** It checks a tag exists, not that the tag is
  right. Read the block.
- **Broken links written as JSX `href=`** rather than markdown.
- **Anything outside `docs/content`.** `/skills` renders
  `docs/site/src/data/skills.json`; redirects live in `docs/site/vercel.json`.
- **Whether a passive construction is worth rewriting.** The grep flags
  candidates; you decide. Flag ones with an obvious active rewrite, not every
  `is` + participle.

Over-matching is the same bug as under-matching. Check what the sweep leaves
alone too, and never report a count you did not just re-run.

## Style rules grep decides poorly

- **Numerals for counts.** The guide wants numerals. Watch for a page that says
  "four parts" in one section and "3 parts" two sections later; the internal
  inconsistency is the stronger argument.
- **First person.** Frontmatter `questions:` in first person is house
  convention across ~370 pages and is **not** a finding. First person in an H2
  body heading is, especially when most headings on the page already use third
  person. Only precedent is 2 headings in
  `sui-stack/zklogin-integration/index.mdx`.
- **Idioms and culturally specific analogies.** Guide says write for non-native
  readers and avoid culturally specific references. Analogies to consumer
  finance, sports, or driving usually fail this.
- **Terminology drift.** "app" vs "application", "onchain" vs "on-chain".
  Compare against sibling pages, not just within the file.
- **Descriptive headings.** "The idea" and "Overview" are weak; the guide wants
  action-based or descriptive.

## Structural checks

- **Prerequisites.** Task pages need the prerequisite tab component, not a prose
  "Before you begin".
- **Frontmatter.** Validate keys against `docs/site/frontmatter.schema.json`.
  Check `has_frontmatter` lists the same fields across sibling pages.
- **Footers.** "Related topics" / "Next steps" are autogenerated. A manual one
  is a finding.
- **Alerts.** Max 4 per page. Prefer `:::tip` / `:::info` over `:::note`.
- **Sidebar.** New entries match surrounding indentation. Note pre-existing
  drift as pre-existing.

## Link verification

```bash
for u in <paths>; do
  printf "%-55s " "$u"
  curl -s -o /dev/null -w "%{http_code} %{redirect_url}\n" "https://docs.sui.io$u"
done
```

A destination that redirects again, or 404s, is a finding. A 404 from `curl` is
not proof a page is unpublished — check `x-vercel-cache` and `age`, cross-check
`sitemap.xml`. Docusaurus serves `/x`, never `/x/index`.

## The generated-tree false positive

`audit-docs.mjs` reports links into `docs/content/references/framework/**` as
broken whenever the local generated copy predates the code. **Verify with
`curl` before reporting.** The tree is generated and gitignored
(`.gitignore:78`); a stale local snapshot is not a docs defect.

Always run `git check-ignore -v <path>` before treating any file as
hand-written. A finding in generated content is still a finding, but it routes
upstream.
