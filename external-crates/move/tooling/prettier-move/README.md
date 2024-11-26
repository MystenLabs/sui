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

Requires [nodejs 18+](https://nodejs.org/en) installed.

## Usage (Global, CLI)

For CLI usage, you can install the plugin globally by running the following command:

```bash
npm i -g prettier @mysten/prettier-plugin-move
```

Then there will be a registered executable `prettier-move` which works exactly like a regular `prettier` one, except that it automatically inserts the path to the plugin as an argument.

```bash
prettier-move -c sources/example.move # to check
prettier-move -w sources/example.move # to write
```

This command is identical to the following:

```bash
prettier --plugin /path/to/local/npm/node_modules/@mysten/prettier-plugin-move/out/index.js -c sources/example.move # to check
prettier --plugin /path/to/local/npm/node_modules/@mysten/prettier-plugin-move/out/index.js -w sources/example.move # to write
```

## Installation (Per-Project)

If you decide to use the plugin per-project, you can install it in the project's directory. This way,
the plugin will be available via `prettier` call in the project's directory.

```bash
# install as a dev-dependency
npm i -D prettier @mysten/prettier-plugin-move
```

Add the `.prettierrc` or a similar configuration file (see [all supported formats](https://prettier.io/docs/en/configuration.html)):

```json
{
	"printWidth": 100,
	"tabWidth": 4,
	"useModuleLabel": true,
	"autoGroupImports": "module",
	"plugins": ["@mysten/prettier-plugin-move"]
}
```

Then you can run prettier either via adding a script to `package.json`:

```json
{
	"scripts": {
		"prettier": "prettier --write ."
	}
}
```

```bash
npm run prettier -w sources/example.move
```

Or, if you have prettier installed globally, you can run it directly:

```bash
prettier --write sources/example.move
```

## VSCode integration

There is a bundled [Move Formatter](https://marketplace.visualstudio.com/items?itemName=mysten.prettier-move) extension for VSCode. It will detect prettier configuration for the workspace and use
the plugin automatically.

Alternatively, if you follow the per-project installation, [regular Pretter extension](https://marketplace.visualstudio.com/items?itemName=esbenp.prettier-vscode) should work as well.

## Known Integrations

- Neovim: see [this commit](https://github.com/amnn/nvim/commit/26236dc08162b61f95f689e232a5df2418708339) for configuration

## Contribute

See [CONTRIBUTING](./CONTRIBUTING.md).
