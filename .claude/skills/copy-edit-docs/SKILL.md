# Copy Edit Documentation

Reviews and fixes documentation files for style consistency.

## Usage

```
/copy-edit-docs [file or directory]
```

Example: `/copy-edit-docs docs/content/guides/developer/my-guide.mdx`
Example: `/copy-edit-docs docs/content/guides/developer/`

## Arguments

$ARGUMENTS should contain either:
- A path to a specific documentation file to review
- A path to a directory containing documentation files

## Style Guidelines

### 1. No License Headers in MDX Files

MDX files do not need license headers. They will mess up rendering.

**No:**
```mdx
{/* Copyright (c) Mysten Labs, Inc. */}
{/* SPDX-License-Identifier: Apache-2.0 */}

---
title: My Guide
---
```

**Yes:**
```mdx
---
title: My Guide
---
```

### 2. Sentence Case for Headings

Use sentence case (only first word and proper nouns capitalized), not title case.

**No:**
```mdx
## Funding Transactions from Address Balances

### Before: Coin Selection

### After: Address Balance Withdrawals
```

**Yes:**
```mdx
## Funding transactions from address balances

### Before: Coin selection

### After: Address balance withdrawals
```

### 3. Periods at End of List Items

Numbered and bulleted list items should end with periods.

**No:**
```mdx
1. **Query owned coins**: Call `suix_getCoins` to retrieve coins
2. **Select coins**: Implement coin selection logic
3. **Handle references**: Include the exact `ObjectRef` for each coin
```

**Yes:**
```mdx
1. **Query owned coins**: Call `suix_getCoins` to retrieve coins.
2. **Select coins**: Implement coin selection logic.
3. **Handle references**: Include the exact `ObjectRef` for each coin.
```

### 4. Backticks for Code in Link Text

When referencing code (function names, types, file paths) in link text, wrap them in backticks.

**No:**
```mdx
[TypeScript SDK - CoinWithBalance](https://example.com):

[Source: sui-types/src/transaction.rs - FundsWithdrawalArg](https://github.com/...):

[TypeScript SDK - Transaction.transferObjects](https://example.com):
```

**Yes:**
```mdx
[TypeScript SDK - `coinWithBalance`](https://example.com):

[Source: `sui-types/src/transaction.rs` - `FundsWithdrawalArg`](https://github.com/...):

[TypeScript SDK - `Transaction.transferObjects`](https://example.com):
```

### 5. Sidebar Entry Required

New documentation pages must have an entry in the corresponding sidebar file to appear in navigation.

- Sidebar files are located in `docs/content/sidebars/`
- The main guides sidebar is `docs/content/sidebars/guides.js`
- Add new pages to the appropriate category in the sidebar

### 6. Frontmatter Requirements

MDX files should include proper frontmatter with at least `title` and `description`. Keywords are recommended for SEO.

**No:**
```mdx
---
title: My Guide
---
```

**Yes:**
```mdx
---
title: My Guide
description: A brief description of what this guide covers.
keywords: [keyword1, keyword2, relevant terms]
---
```

## Instructions

1. Read the target file(s)
2. Check each style guideline above
3. Report any violations found with line numbers
4. Offer to fix the violations automatically
5. If fixing, make all corrections and summarize changes made

## Output Format

When reporting violations:

```
## Style Violations Found

### [filename]

- Line X: [guideline violated] - [specific issue]
- Line Y: [guideline violated] - [specific issue]

### Summary
- N heading case violations
- N missing periods in lists
- N code formatting issues in links
```
