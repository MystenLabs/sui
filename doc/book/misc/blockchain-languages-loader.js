// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Blockchain Languages Loader
 * Registers all blockchain programming languages with HighlightJS
 */

(function() {
    'use strict';

    // Import language definitions
    const moveDefiner = require('./move.js').definer;
    const solidityDefiner = require('./solidity.js').definer;
    const vyperDefiner = require('./vyper.js').definer;
    const cairoDefiner = require('./cairo.js').definer;
    const inkDefiner = require('./ink.js').definer;
    const clarityDefiner = require('./clarity.js').definer;
    const motokoDefiner = require('./motoko.js').definer;

    /**
     * Register all blockchain languages with hljs instance
     * @param {Object} hljs - HighlightJS instance
     */
    function registerBlockchainLanguages(hljs) {
        // Register custom language definitions
        hljs.registerLanguage('move', moveDefiner);
        hljs.registerLanguage('solidity', solidityDefiner);
        hljs.registerLanguage('vyper', vyperDefiner);
        hljs.registerLanguage('cairo', cairoDefiner);
        hljs.registerLanguage('ink', inkDefiner);
        hljs.registerLanguage('clarity', clarityDefiner);
        hljs.registerLanguage('motoko', motokoDefiner);

        // Note: Rust, Haskell, and Go are built-in to HighlightJS
        // They are automatically available and don't need registration

        console.log('Blockchain languages registered:', [
            'Move (Aptos, Sui)',
            'Solidity (Ethereum, EVM)',
            'Vyper (Ethereum)',
            'Rust (Solana, NEAR)',
            'Cairo (StarkNet)',
            'Ink! (Polkadot, Substrate)',
            'Clarity (Stacks, Bitcoin L2)',
            'Motoko (DFINITY, ICP)',
            'Haskell (Cardano)',
            'Go (Cosmos SDK)'
        ]);
    }

    // Export for use in other modules
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = registerBlockchainLanguages;
    }

    // Auto-register if hljs is globally available
    if (typeof hljs !== 'undefined') {
        registerBlockchainLanguages(hljs);
    }

    // Also make it available on window for browser usage
    if (typeof window !== 'undefined') {
        window.registerBlockchainLanguages = registerBlockchainLanguages;
    }
})();
