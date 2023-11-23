# Move

Provides language support for the Move programming language. For information about Move visit the
language [documentation](https://docs.sui.io/concepts/sui-move-concepts).

# How to Install

1. Open a new window in any Visual Studio Code application version 1.55.2 or greater.
2. Open the command palette (`⇧⌘P` on macOS, or use the menu item *View > Command Palette...*) and
   type **Extensions: Install Extensions**. This will open a panel named *Extensions* in the
   sidebar of your Visual Studio Code window.
3. In the search bar labeled *Search Extensions in Marketplace*, type **Move**. The Move extension 
   should appear as one of the option in the list below the search bar. Click **Install**.
4. Open any file that ends in `.move`.

# Troubleshooting

Check [Sui Developer Forum](https://forums.sui.io/c/technical-support) to see if the problem
has already been reported and, if not, report it there.

# Features

Here are some of the features of the Move Visual Studio Code extension. To see them, open a
Move source file (a file with a `.move` file extension) and:

- See Move keywords and types highlighted in appropriate colors.
- Comment and un-comment lines of code using the `⌘/` shortcut on macOS (or the menu command *Edit >
  Toggle Line Comment*).
- Place your cursor on a delimiter, such as `<`, `(`, or `{`, and its corresponding delimiter --
  `>`, `)`, or `}` -- will be highlighted.
- As you type, Move keywords will appear as completion suggestions.
- If the opened Move source file is located within a buildable project (a `Move.toml` file can be
  found in one of its parent directories), the following advanced features will also be available:
  - compiler diagnostics
  - go to definition
  - go to type definition
  - go to references
  - type on hover
  - outline view showing symbol tree for Move source files
