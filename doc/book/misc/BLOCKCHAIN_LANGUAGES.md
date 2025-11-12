# Blockchain Programming Languages Support

This directory contains syntax highlighting definitions and configuration for various blockchain programming languages supported in the Sui documentation and explorer.

## Supported Languages

### 1. **Move** (Aptos, Sui)
- **File**: `move.js`
- **Extensions**: `.move`
- **Description**: Resource-oriented smart contract language designed for the Sui and Aptos blockchains
- **Key Features**: Linear types, resource safety, formal verification support

### 2. **Solidity** (Ethereum, EVM)
- **File**: `solidity.js`
- **Extensions**: `.sol`
- **Description**: The most popular smart contract language for Ethereum and EVM-compatible blockchains
- **Key Features**: Object-oriented, statically typed, supports inheritance and libraries

### 3. **Vyper** (Ethereum)
- **File**: `vyper.js`
- **Extensions**: `.vy`
- **Description**: Python-inspired smart contract language focused on security and simplicity
- **Key Features**: Pythonic syntax, bounds checking, no class inheritance for simplicity

### 4. **Rust** (Solana, NEAR, Ink!)
- **Built-in**: HighlightJS native support
- **Extensions**: `.rs`
- **Description**: Systems programming language used for high-performance blockchain applications
- **Key Features**: Memory safety, zero-cost abstractions, no garbage collection
- **Blockchains**:
  - **Solana**: High-performance blockchain using Rust for smart contracts
  - **NEAR**: Sharded blockchain with Rust support
  - **Ink!**: Rust-based smart contract language for Polkadot/Substrate

### 5. **Cairo** (StarkNet)
- **File**: `cairo.js`
- **Extensions**: `.cairo`
- **Description**: Language for writing provable programs on StarkNet (L2 scaling solution)
- **Key Features**: STARK proofs, ZK-rollup support, provable computation

### 6. **Ink!** (Polkadot, Substrate)
- **File**: `ink.js`
- **Extensions**: `.rs`
- **Description**: Rust-based eDSL for writing Wasm smart contracts on Substrate chains
- **Key Features**: Rust ecosystem, compile to WebAssembly, Polkadot parachain support

### 7. **Clarity** (Stacks, Bitcoin L2)
- **File**: `clarity.js`
- **Extensions**: `.clar`
- **Description**: Decidable language for Bitcoin smart contracts via Stacks
- **Key Features**: No compiler, decidable (no Turing completeness), Bitcoin security

### 8. **Motoko** (DFINITY, Internet Computer)
- **File**: `motoko.js`
- **Extensions**: `.mo`
- **Description**: Actor-based language designed specifically for the Internet Computer
- **Key Features**: Actor model, async/await, automatic persistence

### 9. **Haskell** (Cardano)
- **Built-in**: HighlightJS native support
- **Extensions**: `.hs`
- **Description**: Functional programming language used for Plutus smart contracts on Cardano
- **Key Features**: Strong typing, lazy evaluation, formal verification capabilities

### 10. **Go** (Cosmos SDK)
- **Built-in**: HighlightJS native support
- **Extensions**: `.go`
- **Description**: Used for building custom blockchains with Cosmos SDK
- **Key Features**: Fast compilation, built-in concurrency, simple syntax

## File Structure

```
doc/book/misc/
├── blockchain-languages.json       # Configuration file with all language metadata
├── blockchain-languages-loader.js  # Language loader for HighlightJS
├── move.js                         # Move language definition
├── solidity.js                     # Solidity language definition
├── vyper.js                        # Vyper language definition
├── cairo.js                        # Cairo language definition
├── ink.js                          # Ink! language definition
├── clarity.js                      # Clarity language definition
├── motoko.js                       # Motoko language definition
└── BLOCKCHAIN_LANGUAGES.md         # This file
```

## Integration

### HighlightJS (Documentation)

The language definitions are designed to work with HighlightJS for syntax highlighting in documentation:

```javascript
// Load the blockchain languages loader
const registerBlockchainLanguages = require('./blockchain-languages-loader.js');

// Register all languages with your hljs instance
registerBlockchainLanguages(hljs);

// Use in your code
hljs.highlightBlock(codeBlock);
```

### Prism (Explorer UI)

The ModuleView component in the explorer automatically detects and highlights code in various blockchain languages:

```typescript
import ModuleView from '@/components/module/ModuleView';

// Automatic language detection
<ModuleView itm={[moduleName, code]} />

// Manual language specification
<ModuleView itm={[moduleName, code]} language="solidity" />
```

## Language Detection

The ModuleView component includes smart language detection based on:

1. **File extension** (`.sol`, `.move`, `.vy`, etc.)
2. **Code patterns** (keywords, syntax structures)
3. **Blockchain-specific markers** (pragmas, decorators, etc.)

### Detection Examples

- **Move**: Detects `module`, `fun`, `struct` with `has` keyword
- **Solidity**: Detects `pragma solidity`, `contract` keyword
- **Vyper**: Detects `@external`, `@internal` decorators
- **Cairo**: Detects `%lang starknet`, `felt` types
- **Clarity**: Detects `(define-` Lisp-style syntax
- **Ink!**: Detects `#[ink(` attribute macros

## Adding New Languages

To add support for a new blockchain language:

1. Create a new syntax definition file (e.g., `newlang.js`) following the existing format
2. Add the language to `blockchain-languages.json`
3. Import and register in `blockchain-languages-loader.js`
4. Add detection logic in `ModuleView.tsx` if needed
5. Test syntax highlighting with sample code

## Example Language Definition

```javascript
// newlang.js
function hljsDefineNewLang(hljs) {
    var KEYWORDS = 'keyword1 keyword2 keyword3';
    var BUILTINS = 'builtin1 builtin2 builtin3';

    return {
        name: 'NewLang',
        aliases: ['newlang'],
        keywords: {
            keyword: KEYWORDS,
            literal: 'true false',
            built_in: BUILTINS
        },
        contains: [
            hljs.C_LINE_COMMENT_MODE,
            hljs.QUOTE_STRING_MODE,
            // ... other patterns
        ]
    };
}

module.exports = function(hljs) {
    hljs.registerLanguage('newlang', hljsDefineNewLang);
};

module.exports.definer = hljsDefineNewLang;
```

## Configuration Schema

The `blockchain-languages.json` file follows this schema:

```json
{
  "languages": [
    {
      "name": "Language Name",
      "aliases": ["alias1", "alias2"],
      "description": "Language description",
      "blockchain": ["Blockchain1", "Blockchain2"],
      "file": "filename.js or 'builtin'",
      "extensions": [".ext"],
      "enabled": true,
      "note": "Optional note"
    }
  ],
  "metadata": {
    "version": "1.0.0",
    "last_updated": "2025-11-12",
    "description": "Configuration metadata"
  }
}
```

## Contributing

When contributing new language support:

1. Ensure syntax definitions are accurate and comprehensive
2. Test with real-world code examples
3. Update this documentation
4. Add detection patterns that avoid false positives
5. Consider edge cases and dialect variations

## Resources

### Official Language Documentation

- **Move**: https://move-language.github.io/move/
- **Solidity**: https://docs.soliditylang.org/
- **Vyper**: https://docs.vyperlang.org/
- **Rust**: https://www.rust-lang.org/
- **Cairo**: https://www.cairo-lang.org/docs/
- **Ink!**: https://use.ink/
- **Clarity**: https://docs.stacks.co/clarity/
- **Motoko**: https://internetcomputer.org/docs/current/motoko/main/motoko/
- **Haskell**: https://www.haskell.org/
- **Go**: https://go.dev/

### Blockchain Platforms

- **Sui**: https://sui.io/
- **Ethereum**: https://ethereum.org/
- **Solana**: https://solana.com/
- **StarkNet**: https://starknet.io/
- **Polkadot**: https://polkadot.network/
- **Stacks**: https://www.stacks.co/
- **Internet Computer**: https://internetcomputer.org/
- **Cardano**: https://cardano.org/
- **Cosmos**: https://cosmos.network/

## License

Copyright (c) 2022, Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
