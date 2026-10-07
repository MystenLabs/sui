# The Sui documentation has moved

The pages behind [docs.sui.io](https://docs.sui.io) are no longer in this
repository. They now live in
**[MystenLabs/sui-docs](https://github.com/MystenLabs/sui-docs)**, along with the
Docusaurus site that renders them. The site itself is unchanged: same URLs, same
content.

| What you want | Where it is now |
| --- | --- |
| A documentation page | [`content/sui/`](https://github.com/MystenLabs/sui-docs/tree/main/content/sui) |
| The site, its theme and plugins | [`sites/sui/`](https://github.com/MystenLabs/sui-docs/tree/main/sites/sui) |
| To report a problem with the docs | [Open an issue](https://github.com/MystenLabs/sui-docs/issues/new) on `sui-docs` |
| To fix a page yourself | The **Edit this page** link at the bottom of any page on docs.sui.io, or a pull request against `sui-docs` |

## What is still here, and why

Parts of the documentation are generated at build time from source in this
repository, and `sui-docs` fetches them on every build:

| Path | Generates |
| --- | --- |
| `crates/sui-framework/docs/` | the Move framework reference |
| `crates/`, `examples/` | the code embedded in pages by `ImportContent` |
| `release-notes/` | the release notes page |

Two things follow.

Comments marked `docs::#name` in Rust and Move source are **snippet anchors**, not
ordinary comments. A page quotes the lines between them, so renaming or removing
one breaks whatever page embeds it.

Moving or renaming any of the paths above breaks the documentation build in the
other repository, and nothing here will tell you. If you need to move one,
change
[`sources.json`](https://github.com/MystenLabs/sui-docs/blob/main/sources.json)
in `sui-docs` in the same change.

The awesome-sui lists used to be vendored here as a git subtree under
`docs/subtree`. `sui-docs` now fetches them from
[sui-foundation/awesome-sui](https://github.com/sui-foundation/awesome-sui) and
[becky-sui/awesome-sui-gaming](https://github.com/becky-sui/awesome-sui-gaming)
directly, so there is nothing to keep in sync here.

## Why it moved

The machinery underneath seven documentation sites had been copied rather than
shared: the same fetcher written three times, the same frontmatter schema in
three repositories, shared components kept aligned across four repositories by a
sync bot. `sui-docs` gives that one home.
