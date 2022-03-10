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
        link: '/key-concepts/',
      },
      {
        label: 'Dynamic properties',
        link: '/key-concepts/dynamic-properties/',
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
        fileName: 'what-makes-sui-different',
      },
      {
        label: 'Terminology',
        fileName: 'move',
      },
      {
        label: 'Why Move?',
        fileName: 'what-makes-sui-different',
      },
      {
        label: 'Why Sui?',
        fileName: 'what-makes-sui-different',
      },
      {
        label: 'What Makes Sui Different?',
        fileName: 'what-makes-sui-different',
      },
      {
        /// Items here are submenu items under the learn menu
        title: 'How Sui works',
        items: [
          {
            label: 'Security',
            fileName: 'sui-json',
          },
          {
            label: 'Storage',
            fileName: 'sui-json',
          },
          {
            label: 'Limitations',
            fileName: 'sui-json',
          },
        ],
      },
      {
        label: 'Sui White Paper',
        fileName: 'what-makes-sui-different',
      },
    ],
    build: [
      {
        label: 'Install',
        fileName: 'what-makes-sui-different',
      },

      {
        label: 'Smart Contracts with Move',
        fileName: 'what-makes-sui-different',
      },
      {
        label: 'Wallet',
        fileName: 'wallet',
      },
      {
        label: 'Objects',
        fileName: 'object',
      },
      {
        label: 'Transactions',
        fileName: 'transactions',
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
            fileName: 'sui-json',
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
        link: 'https://www.google.com/',
        external: true,
      },
    ],
  },
}
