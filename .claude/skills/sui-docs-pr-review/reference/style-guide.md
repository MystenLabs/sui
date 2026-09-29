# Documentation Style Guide

This file consolidates the style rules that govern all documentation output from this skill. It draws from the [Sui Documentation Style Guide](https://github.com/MystenLabs/sui/blob/main/docs/sui-documentation-style-guide.skill).

---

## Dependent skills

This skill works alongside other installed skills. Load the appropriate skill when its domain applies:

| Dependency | Skill | Load when |
|---|---|---|
| Sui style guide | `sui-documentation-style-guide` | Applying Sui-specific editorial rules (install from [MystenLabs/sui](https://github.com/MystenLabs/sui/blob/main/docs/sui-documentation-style-guide.skill)) |
| Walrus overview | `walrus-overview` | Documenting Walrus concepts or architecture |
| Walrus CLI | `walrus-cli` | Documenting Walrus CLI usage |
| Walrus HTTP API | `walrus-http-api` | Documenting Walrus REST API usage |
| Walrus TypeScript SDK | `walrus-ts-sdk` | Documenting Walrus TypeScript SDK usage |
| Walrus Sites | `walrus-sites` | Documenting Walrus Sites deployment |
| Walrus storage costs | `walrus-storage-costs` | Documenting Walrus pricing and cost models |
| Walrus data security | `walrus-data-security` | Documenting encryption with Seal |
| Sui Move | `sui-move` | Documenting Move smart contract concepts |
| Sui object model | `sui-object-model` | Documenting object ownership, dynamic fields, collections |
| Sui PTBs | `ptbs` | Documenting programmable transaction blocks |
| Sui SDKs | `sui-sdks` | Documenting SDK selection and setup |
| Sui TypeScript patterns | `sui-ts-docs-patterns` | Writing TypeScript code examples for docs |
| Frontend apps | `frontend-apps` | Documenting dApp Kit usage |
| Data access | `accessing-data` | Documenting data access strategies and APIs |

---

## Editorial principles

- Use plain, direct language. Short sentences. Write for non-native English speakers.
- Do not start introductions with "this page" or "this guide." Lead with the topic itself for better answer engine optimization (AEO) and generative engine optimization (GEO).
- Do not redefine common words or use jargon, slang, or idioms.
- Introduce technical terms only when necessary. Define on first use, then use consistently.
- Be explicit: "Deploy the contract" not "do the thing."
- Write for a global audience. Favor clarity over cleverness. Avoid culturally specific references.
- Consider AI ingestion: documentation pages are ingested by agents and chatbots. Ensure content is parseable as markdown.
- Keep finished pages under 50,000 characters. Leave room for human revisions and additions.

## Spelling and grammar

- **US English** spelling.
- **No Latin abbreviations:** No "e.g.", "i.e.", "etc.", "et al." Use "for example", "and so on", or "ex."
- **Active voice** always. Rewrite so the subject performs the action.
- **Second person ("you").** Never first person ("I"/"we") or third person.
- **Present tense** always. No future tense for product behavior or instructions.
- **Oxford commas:** Always use serial commas.
- **Numbers:** Use numerals for counts (7 files, 24 items). Write out numbers only when grammatically part of the sentence.
- **No quotation marks** (exception: "Hello, World!"). Use backticks for error messages.
- **No ampersands** in prose. Use "and".
- **No exclamation marks.**
- **No em dashes** in prose. Rewrite using commas, parentheses, or split sentences. Em dashes inside code blocks and CLI commands are fine.

## Terminology

Follow the Sui Documentation Style Guide terminology rules exactly. Key rules:

### Always capitalized (in prose)
Sui, Mainnet, Testnet, Devnet, Localnet, SUI, WAL, DeepBook, GraphQL RPC, One-Time Witness, Sui dApp Kit, Walrus Foundation, and all product names listed in the style guide.

### Always lowercase (in prose)
gas, epoch, object, oracle, smart contract, transaction, transfer, validator, wallet, onchain, offchain.

### Never hyphenated
key pair, layer 1, offchain, onchain, open source, use case.

### Word preferences
| Instead of | Use |
|---|---|
| may | might |
| "Please note" / "Note" at start | (remove or rewrite) |
| via | through |
| since (causal) | because |
| simple | basic |
| dApp | app (except "dApp Kit" product name) |

**Code and URLs are untouchable:** Terminology rules apply to prose only. Never apply them to code blocks, inline code, CLI commands, or URLs.

## Capitalization

- **Page titles:** Title case. Do not capitalize short conjunctions/prepositions unless first or last word.
- **Section headings:** Sentence case.
- **Body text:** Capitalize first word of sentences and proper nouns/product names. No ALL CAPS for emphasis (use bold).

## Body text styling

- **Bold:** Term-definition pairs (bold the term before the colon). UI elements. Port references.
- **No italic text.**
- **Variables:** Uppercase with underscores for placeholders: `NETWORK_NAME`, `YOUR_API_KEY`.

The bold list above says where bold is *called for* in body text. It is not an
exhaustive whitelist, and bold outside those cases is not a violation.

## Tables

Bold header rows (`| **Column** |`) are correct and widely used. Do not flag
them. This guide does not otherwise constrain table formatting.

## Titles and headings

- Use descriptive, action-based titles ("Using Packages" not "Package Overview").
- Keep headings concise.
- Never stack headings without body text between them.
- Heading hierarchy: H1 for page title only, H2 for top-level sections, H3 for sub-topics, H4 for short-form content.

## Lists

- Introduce lists with a description ending in a colon.
- Use lists instead of serial comma sentences with 4 or more items.
- **Numbered lists** for sequences.
- **Bulleted lists** for related items. Periods only on full sentences.
- **Term lists:** Bold term, colon, definition.

## Code in documentation

- **Inline code:** Backticks around object names, function names, file names, CLI commands, variable names, file paths.
- **Console commands:** Triple backticks, start with `$`. Keep commands and output in separate blocks.
- **Source from GitHub** using `<ImportContent>` component instead of copying inline. See `source-code-policy.md` for the full policy.

## Prerequisites

Use the prerequisite tab component. Do not use prose headings like "Before you begin."

For Sui docs:
```mdx
<Tabs className="tabsHeadingCentered--small">
<TabItem value="prereq" label="Prerequisites">
- [x] Prerequisite one with [inline link](/path)
- [x] Prerequisite two with [inline link](/path)
</TabItem>
</Tabs>
```

For Walrus docs:
```mdx
<div className="outlined-tabs">
<Tabs>
<TabItem value="prereq" label="Prerequisites">
- [x] Prerequisite one with [inline link](/path)
- [x] Prerequisite two with [inline link](/path)
</TabItem>
</Tabs>
</div>
```

## Links and references

- Use full relative links for docs.sui.io topics.
- Never use a bare URL as link text.
- **Crosslink Sui concepts on first mention** to their docs page. Do not repeat the link on subsequent mentions.

## Alerts (admonitions)

Maximum 4 per page. All alert content must be complete sentences:

- **`:::caution`** — Risk of data loss, errors, or breaking changes.
- **`:::danger`** — Critical or irreversible consequences.
- **`:::info`** — Important neutral context or conditions.
- **`:::tip`** — Best practices, shortcuts, helpful advice.

Avoid `:::note`. Prefer `:::tip` or `:::info`.

## Footer sections

Do not manually add "Related topics", "Next steps", or "Related links" footer sections. These are autogenerated by the site build.

## Accessibility

- No color or special symbols for emphasis.
- Alt text and captions on all images.
- Images never substitute for text content.

## Banned API references

**Never reference or use JSON-RPC anywhere in documentation.** JSON-RPC is deprecated. Use [GraphQL RPC](https://docs.sui.io/develop/graphql-rpc) or [gRPC](https://docs.sui.io/develop/grpc) instead. If existing documentation references JSON-RPC, flag it for removal and replace with the appropriate modern API.
