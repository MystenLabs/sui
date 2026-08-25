# Prettier Move Tests

This directory contains tests for the package.

## Running Tests

To run the tests, you can use the following command:

```bash
pnpm test
```

To regenerate expectations, you can use the following command:

```bash
UB=1 pnpm test
```

## Test Structure

The tests should be placed as `.move` files in the subdirectories of this directory (one level deep — files directly in this directory are not picked up). Each test is compared against its expectation file (`<test>.exp.move`); running with `UB=1` (re)generates it. Ideally, tests should cover all of the CST nodes defined in the `grammar.json`.

## Special Features

You can customize behaviour of prettier for a specific file by adding a comment at the top. If the file starts with `// options:`, the test runner will attempt to read the following lines as options for prettier. For example:

```move
// options:
// tabWidth: 4
// printWidth: 40

module prettier::test {}
```

Supported options: `printWidth`, `tabWidth`, `wrapComments`, `useModuleLabel`,
`autoGroupImports` (`package` / `module` / `none`), and `enableErrorDebug`.
