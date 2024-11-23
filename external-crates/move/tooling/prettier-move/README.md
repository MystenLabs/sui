# Prettier Move Plugin

This is a Move language plugin for the
[Prettier](https://prettier.io/) code formatter. It uses a Move
[parser](https://github.com/tzakian/tree-sitter-move) built on top of the
[tree-sitter](https://tree-sitter.github.io/) parser generator and maintained by Tim Zakian.

The plugin is platform-independent by utilizing a [WASM](https://webassembly.org/)-ified version of
the Move parser included in this repository at
[(./tree-sitter-move.wasm)](./tree-sitter-move.wasm). You can re-generate the WASM-ified version of
the parser by running the [scripts/treesitter-wasm-gen.sh](scripts/treesitter-wasm-gen.sh) script
(prerequisites for this are listed in the script itself). You should be careful when doing so, as
certain changes to the parser may break the plugin (e.g., if parse tree node types are modified).

## Prerequisites

In order to use the plugin, you need to install `npm` command (`brew install npm` on a
Mac). You can use the plugin to format Move files (`.move` extension) both on the command line and
using Prettier's VSCode
[extension](https://marketplace.visualstudio.com/items?itemName=esbenp.prettier-vscode). When the
plugin is complete, we will make it available directly from Move's VSCode extension.

## Usage (with Npm)

1. To use the plugin, you can install it as a development dependency in your Move package:

> This will install both the prettier formatter and the plugin in the `./node_modules` directory.

```sh
npm install -D prettier @mysten/prettier-plugin-move
```

1. Create a `.prettierrc` file containing the following configuration:

```json
{
  "plugins": ["@mysten/prettier-plugin-move"]
}
```

3. Add a script in your `package.json` file to format Move files:

```json
"scripts": {
  "prettier": "prettier --write ."
}
```

Now you can format all Move files in your package by running:

```sh
npm run prettier
```

## Usage (with Npx)

This will require having the prettier-move plugin built (`tsc`) locally in the SUI repository.

```sh
npx --yes prettier@3.3.2 --plugin "$SUI/external-crates/move/tooling/prettier-move/out/index.js" --write "path/to/move/folder/**/*.move"
```

# VSCode integration

In order to use the plugin in VSCode you first need to install

-   Prettier's VSCode [extension](https://marketplace.visualstudio.com/items?itemName=esbenp.prettier-vscode).
-   Move's formatter [extension](https://marketplace.visualstudio.com/items?itemName=mysten.prettier-move).

After completing these steps, you will be able to format move files by choosing `Format Code` command from VSCode's command palette.

## Contribute

See [CONTRIBUTING](./CONTRIBUTING.md).
