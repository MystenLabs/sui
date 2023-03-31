---
id: move-book
title: Move Book
custom_edit_url: https://github.com/move-language/move/edit/main/language/documentation/book/README.md
---

In order to update the Move book and preview changes to it you'll need to
install
[`mdbook`](https://rust-lang.github.io/mdBook/guide/installation.html). You
can do this via `cargo install mdbook`.

After installing mdbook, you can preview changes via either `mdbook serve`,
or if you want the output to change as you make changes to the book you can
use `mdbook watch`. More information on options can be found at the [mdbook
website](https://rust-lang.github.io/mdBook/).

Once you are happy with your changes to the Move book, you can create a PR to
update the Move book website. This is the process that has been used in
the past and is known to work, but there may be a better way:

1. Run `mdbook build` in this directory. This will create a directory
called `book`. Copy this to some location `L` outside of the Move git tree.

2. Make sure your upstream is up-to-date and checkout to `upstream/gh-pages`.

3. Once you have checked out to `upstream/gh-pages`, make sure you are at the
root of the repo. You should see a number of `.html` files. You can now
move all contents in `L` to this location: `mv L/* .`

4. Once this is done inspect things to make sure the book looks the way you
want. If everything looks good submit a PR to the **`gh-pages`** branch of the
main Move repo.

After this the normal PR process is followed. Once the PR has been accepted and
lands the updated Move book should be visible immediately.

**NB:** When adding a new (sub)section to the book you _must_ include the new
file in the `SUMMARY.md` file otherwise it will not appear in the updated book.
