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

The tests should be placed as `.move` files in this directory. All `.move` files will be treated as tests, and if there's no expectation file, one will be generated (`<test>.exp.move`). Ideally, tests should cover all of the CST nodes defined in the `grammar.json`.

## Special Features

You can customize behaviour of prettier for a specific file by adding a comment at the top. If the file starts with `// options:`, the test runner will attempt to read the following lines as options for prettier. For example:

```move
// options:
// tabWidth: 4
// printWidth: 40

module prettier::test {}
```

Currently, only 2 options are supported: `tabWidth` and `printWidth`.
