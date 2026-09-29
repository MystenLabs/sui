# Source Code Policy

This skill generates **documentation only**. It never generates, writes, or fabricates source code.

---

## Core rule

**Do not write source code.** This includes Move smart contracts, TypeScript/JavaScript, Rust, Python, shell scripts, configuration files, or code in any other language. Documentation is prose, structure, and references. Code lives in external repositories and is sourced from there.

## Where code comes from

All code that appears in documentation must be sourced from an external repository or verified codebase. Acceptable sources include:

- MystenLabs GitHub repositories (sui, walrus, walrus-docs, sui-typescript, and so on)
- Official example repositories referenced in Sui or Walrus documentation
- User-specified repositories the documentation is being written for

When a documentation page needs a code example, use one of the following approaches:

### 1. `<ImportContent>` component (preferred)

Source code directly from GitHub using the `<ImportContent>` component. This keeps samples in sync with the source and reduces maintenance burden:

```mdx
<ImportContent source="src/lib/example.ts" mode="code" org="MystenLabs" repo="example-repo" />
```

Use targeting attributes (`fun`, `struct`, `variable`, `tag`, and so on) to extract a specific code component rather than the entire file:

```mdx
<ImportContent source="sources/example.move" mode="code" org="MystenLabs" repo="example-repo" fun="transfer_object" />
```

### 2. Placeholder blocks

When the source repository or file path is not yet known, insert a placeholder block that a human author or CI process fills in later:

```mdx
{/* TODO: Import code from REPO_NAME/PATH_TO_FILE — function: FUNCTION_NAME */}
{/* Description: Brief description of what this code demonstrates */}
```

### 3. Referencing existing documentation

When code already exists in published documentation, cross-reference it with a link rather than duplicating it:

```mdx
For the full implementation, see [Creating a Coin](/develop/coins/create-coin).
```

## What is NOT source code

The following are acceptable to write inline because they are documentation artifacts, not source code:

- **CLI commands** demonstrating tool usage (`sui client publish`, `walrus store`)
- **Configuration snippets** showing frontmatter, `Move.toml` fields, or environment variables when explaining documentation structure
- **API request/response examples** showing expected inputs and outputs (use realistic but clearly illustrative data)
- **Pseudocode** in prose form explaining an algorithm or flow conceptually (label clearly as pseudocode)
- **Terminal output** showing expected results of commands

## Citation requirements for code references

When referencing code that appears via `<ImportContent>` or placeholder, include:

- The repository name and organization
- The file path within the repository
- The specific function, struct, or section being referenced
- A brief prose explanation of what the code does and why it matters in context

## Inline source citations

Every technical claim, behavioral description, or architectural assertion must include an inline citation to its source. Acceptable citation formats:

- Link to the specific documentation page: `[Programmable Transaction Blocks](/develop/transactions/ptbs/prog-txn-blocks)`
- Link to source code: `[source](https://github.com/MystenLabs/sui/blob/main/path/to/file.rs)`
- Link to specification or whitepaper: `[Walrus whitepaper](https://docs.wal.app/walrus.pdf)`

Do not make uncited technical claims. If a source cannot be identified, flag the claim with:

```mdx
{/* TODO: Citation needed — verify this claim and add source */}
```
