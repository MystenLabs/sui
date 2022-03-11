/*
  the top level name under the docs object is the name of the nav item on the left
  Use the name of the markdown file in the src folder. If the file is in a subdirectory include the subdirectory
  eg fileName: 'getting-started/installation'
  The title on the SideNav is the title from the FrontMatter of the file. If the file has no FrontMatter the title is the file name without dash or underscore.

*/
module.exports = {
  sideMenu: {
    title: 'MAIN CONCEPTS',
    items: [
      {
        label: 'Selections',
        link: 'learn/how-sui-works',
      },
      {
        label: 'Dynamic properties',
        link: 'key-concepts/dynamic-properties',
      },
      {
        label: 'Events',
        link: 'https://www.google.com/',
        external: true,
      },
    ],
  },
  docs: {
    /// replace all links file name
    learn: [
      {
        label: 'About Sui',
        fileName: 'learn/how-sui-works',
      },
      {
        label: 'Terminology',
        fileName: 'learn/how-sui-works',
      },
      {
        label: 'Why Move?',
        fileName: 'learn/sui-move-diffs',
      },

      {
        label: 'What Makes Sui Different?',
        fileName: 'learn/sui-compared',
      },
      {
        /// Items here are submenu items under the learn menu
        title: 'How Sui works',
        items: [
          {
            label: 'Security',
            fileName: 'learn/sui-compared',
          },
          {
            label: 'Storage',
            fileName: 'learn/sui-compared',
          },
          {
            label: 'Limitations',
            fileName: 'learn/sui-compared',
          },
        ],
      },
      {
        label: 'Sui White Paper',
        fileName: 'learn/how-sui-works',
      },
    ],
    build: [
      {
        label: 'Install',
        fileName: 'build/authorities',
      },

      {
        label: 'Smart Contracts with Move',
        fileName: 'build/move',
      },
      {
        label: 'Wallet',
        fileName: 'build/wallet',
      },
      {
        label: 'Objects',
        fileName: 'build/objects',
      },
      {
        label: 'Transactions',
        fileName: 'build/transactions',
      },
    ],
    explore: [
      {
        /// Items here are submenu items under the learn menu
        title: 'Examples',
        items: [
          {
            label: 'Mint NFT with Additional Mutable Fields',
            fileName: 'sui-json',
          },
          {
            label: 'Transfer, Bundle, and Wrap NFTs',
            fileName: 'sui-json',
          },
          {
            label: 'Use Ethereum NFTs in Sui?',
            fileName: 'sui-json1',
          },
        ],
      },
    ],

    contribute: [
      /// For External links use the following format
      // No Submenu on exteral links
      {
        label: 'Community',
        link: 'https://www.google.com/',
        external: true,
      },
      {
        label: 'Logging',
        link: 'https://www.google.com/',
        external: true,
      },
      {
        label: 'About Mysten Labs',
        link: 'https://www.google.com/',
        external: true,
      },
      {
        label: 'Contributing to Sui',
        link: 'https://www.google.com/',
        external: true,
      },
      {
        label: 'Code of Conduct',
        fileName: 'contribute/code-of-conduct',

      },
    ],
  },
}
