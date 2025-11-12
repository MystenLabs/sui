// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';
import Highlight, { defaultProps, Prism } from 'prism-react-renderer';
import 'prism-themes/themes/prism-one-light.css';

import codestyle from '../../styles/bytecode.module.css';

import type { Language } from 'prism-react-renderer';

import styles from './ModuleView.module.css';

// Setup Prism for blockchain languages
// @ts-ignore
(typeof global !== 'undefined' ? global : window).Prism = Prism;

// Load built-in Prism languages for blockchain development
require('prismjs/components/prism-rust'); // Rust (Solana, NEAR, Ink!)
require('prismjs/components/prism-solidity'); // Solidity (Ethereum, EVM) - if available
require('prismjs/components/prism-python'); // For Vyper (Python-style)
require('prismjs/components/prism-haskell'); // Haskell (Cardano)
require('prismjs/components/prism-go'); // Go (Cosmos SDK)

// Note: Some blockchain languages may need custom Prism definitions:
// - Move (Aptos, Sui) - may use rust-like syntax
// - Cairo (StarkNet) - may use rust-like syntax
// - Vyper - uses python syntax
// - Clarity (Stacks) - lisp-like syntax
// - Motoko (ICP) - may need custom definition

// Blockchain language type mapping
export type BlockchainLanguage =
    | 'move'      // Move (Aptos, Sui)
    | 'solidity'  // Solidity (Ethereum, EVM)
    | 'vyper'     // Vyper (Ethereum, Python-style)
    | 'rust'      // Rust (Solana, NEAR, Ink!)
    | 'cairo'     // Cairo (StarkNet)
    | 'clarity'   // Clarity (Stacks, Bitcoin L2)
    | 'motoko'    // Motoko (DFINITY, ICP)
    | 'haskell'   // Haskell (Cardano)
    | 'go';       // Go (Cosmos SDK)

/**
 * Detect language from code content or module name
 */
function detectLanguage(moduleName: string, code: string): Language {
    const name = moduleName.toLowerCase();
    const codeSnippet = code.substring(0, 200).toLowerCase();

    // Move language detection (Sui, Aptos)
    if (name.endsWith('.move') ||
        codeSnippet.includes('module ') && codeSnippet.includes('::') ||
        codeSnippet.includes('fun ') ||
        codeSnippet.includes('struct ') && codeSnippet.includes('has')) {
        return 'rust' as Language; // Use rust syntax for Move
    }

    // Solidity detection (Ethereum, EVM)
    if (name.endsWith('.sol') ||
        codeSnippet.includes('pragma solidity') ||
        codeSnippet.includes('contract ') && codeSnippet.includes('{')) {
        return 'solidity' as Language;
    }

    // Vyper detection (Ethereum, Python-style)
    if (name.endsWith('.vy') ||
        codeSnippet.includes('@external') ||
        codeSnippet.includes('@internal') ||
        codeSnippet.includes('def ') && codeSnippet.includes(':')) {
        return 'python' as Language;
    }

    // Cairo detection (StarkNet)
    if (name.endsWith('.cairo') ||
        codeSnippet.includes('%lang starknet') ||
        codeSnippet.includes('@external') ||
        codeSnippet.includes('func ') && codeSnippet.includes('felt')) {
        return 'rust' as Language; // Use rust syntax for Cairo
    }

    // Clarity detection (Stacks, Bitcoin L2)
    if (name.endsWith('.clar') ||
        codeSnippet.includes('(define-') ||
        codeSnippet.includes('(contract-call?')) {
        return 'clojure' as Language; // Use clojure/lisp syntax
    }

    // Motoko detection (DFINITY, ICP)
    if (name.endsWith('.mo') ||
        codeSnippet.includes('actor ') ||
        codeSnippet.includes('async ') && codeSnippet.includes('await')) {
        return 'typescript' as Language; // Use typescript-like syntax
    }

    // Rust detection (Solana, NEAR, Ink!)
    if (name.endsWith('.rs') ||
        codeSnippet.includes('#[program]') ||
        codeSnippet.includes('#[ink(') ||
        codeSnippet.includes('pub fn ') ||
        codeSnippet.includes('impl ')) {
        return 'rust' as Language;
    }

    // Haskell detection (Cardano)
    if (name.endsWith('.hs') ||
        codeSnippet.includes('module ') && codeSnippet.includes('where') ||
        codeSnippet.includes('data ') ||
        codeSnippet.includes('::')) {
        return 'haskell' as Language;
    }

    // Go detection (Cosmos SDK)
    if (name.endsWith('.go') ||
        codeSnippet.includes('package ') ||
        codeSnippet.includes('func ') && codeSnippet.includes('(')) {
        return 'go' as Language;
    }

    // Default to rust for bytecode or unknown
    return 'rust' as Language;
}

interface ModuleViewProps {
    itm: [string, string]; // [moduleName, code]
    language?: BlockchainLanguage;
}

function ModuleView({ itm, language }: ModuleViewProps) {
    const [moduleName, code] = itm;
    const detectedLanguage = language || detectLanguage(moduleName, code);

    return (
        <section className={styles.modulewrapper}>
            <div className={styles.moduletitle}>
                {moduleName}
                <span className={styles.languageBadge}>
                    {detectedLanguage}
                </span>
            </div>
            <div className={cl(codestyle.code, styles.codeview)}>
                <Highlight
                    {...defaultProps}
                    code={code}
                    language={detectedLanguage}
                    theme={undefined}
                >
                    {({
                        className,
                        style,
                        tokens,
                        getLineProps,
                        getTokenProps,
                    }) => (
                        <pre className={className} style={style}>
                            {tokens.map((line, i) => (
                                <div
                                    {...getLineProps({ line, key: i })}
                                    key={i}
                                    className={styles.codeline}
                                >
                                    <div className={styles.codelinenumbers}>
                                        {i + 1}
                                    </div>
                                    {line.map((token, key) => (
                                        <span
                                            {...getTokenProps({ token, key })}
                                            key={key}
                                        />
                                    ))}
                                </div>
                            ))}
                        </pre>
                    )}
                </Highlight>
            </div>
        </section>
    );
}

export default ModuleView;
