# Developer Updates

Files in this directory are standalone posts for the [Developer Updates](https://docs.sui.io/developer-updates) page.

Use a Developer Update for a focused post about one thing: a new primitive, a
migration developers need to plan for, a change in recommended practice. Use a
[release note](../README.md) when the content belongs to a specific protocol
release and only makes sense next to that version.

Unlike the version directories alongside this one, posts here are not tied to a
release. They are ordered by date, newest first.

## Adding a post

1. Create a file named `YYYY-MM-DD-short-slug.md`. The date is the publication
   date, and it sets the order on the page.
2. Give it frontmatter:

   ```markdown
   ---
   title: Allowances, a native payments primitive
   date: 2026-09-24
   summary: One or two sentences that introduce the post on the index.
   author: Optional name or team
   ---
   ```

   `title` and `date` are required. `summary` and `author` are optional.

   Add `draft: true` to keep a post in the repository but off the page. Use it
   to write and review a post before the thing it describes ships, then remove
   the line to publish.

3. Write the body in markdown, starting at `##`. Do not repeat the title as a
   heading, because the generator adds it.
4. Open a pull request. The page regenerates on the next site build.

## How it reaches the site

`docs/site/scripts/convert-developer-updates.cjs` reads this directory and
writes `docs/content/developer-updates.mdx`. The site build runs it through the
`prebuild` and `prestart` scripts in `docs/site/package.json`, so you do not run
anything by hand.

That generated page is not in version control. Your post here is the only copy,
so edit it here and never edit `developer-updates.mdx` directly. Any change to
the generated page is overwritten on the next build.

To preview locally:

```sh
$ cd docs/site
$ pnpm start
```

## Conventions

- Write in the present tense, in second person, and follow the
  [Sui documentation style guide](https://docs.sui.io/references/contribute/style-guide).
- Link to reference documentation rather than restating it. A post explains what
  changed and why it matters; the docs explain how the API works.
- Date a post for the day it goes live, not the day you write it.
