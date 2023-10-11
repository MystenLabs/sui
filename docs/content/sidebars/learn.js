const learn = [
    {
        type: 'doc',
        id: 'learn',
        label: 'Learn',
      },
        {
        type: 'category',
        label: 'Sui Overview',
        link: {
            type: 'doc',
            id: 'learn/sui-overview/sui-overview',
        },
        items: [
            'learn/sui-overview/sui-overview',
            'learn/sui-overview/why-move',
            'learn/sui-overview/how-sui-move-differs',
        ],
        },
        {
        type: 'category',
        label: 'Core Concepts',
        link: {
            type: 'doc',
            id: 'learn/core-concepts/how-sui-works',
        },
        items: [
            'learn/core-concepts/how-sui-works',
            'learn/core-concepts/objects',
            'learn/core-concepts/object-and-package-versioning',
            'learn/core-concepts/transactions',
            'learn/core-concepts/sponsored-transactions',
            'learn/core-concepts/single-writer-apps',
            'learn/core-concepts/validators',
            'learn/core-concepts/consensus-engine',
        ],
        },
        {
        type: 'category',
        label: 'Economics',
        link: {
            type: 'doc',
            id: 'learn/economics/sui-tokenomics',
        },
        items: [
            'learn/economics/sui-tokenomics',
            'learn/economics/sui-token',
            'learn/economics/gas-pricing',
            'learn/economics/gas-in-sui',
            'learn/economics/sui-storage-fund',
            'learn/economics/proof-of-stake',
            {
            type: 'link',
            label: 'Sui Whitepaper',
            href: 'https://github.com/MystenLabs/sui/blob/main/doc/paper/tokenomics.pdf/',
            },
        ],
        },
        {
        type: 'category',
        label: 'Cryptography',
        link: {
            type: 'doc',
            id: 'learn/cryptography/cryptography',
        },
        items: [
            'learn/cryptography/cryptography',
            'learn/cryptography/keys-and-addresses',
            'learn/cryptography/signatures',
            'learn/cryptography/multisig',
            'learn/cryptography/offline-signing',
            'learn/cryptography/intent-signing',
            'learn/cryptography/schemes',
            'learn/cryptography/hashing',
            'learn/cryptography/groth16',
            'learn/cryptography/ecvrf',
        ]
        }
    ]

module.exports = learn;