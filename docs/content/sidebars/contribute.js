const contribute = [
    {
        type: "doc",
        id: "contribute",
        label: "Contribute"
    },
    {
      type: 'category',
      label: 'Contribute',
      link: {
        type: 'doc',
        id: 'contribute/contribution-process',
      },
      items: [
        'contribute/contribution-process',
        'contribute/contribute-to-sui-repos',
        {
          type: 'link',
          label: 'Submit a SIP',
          href: 'https://sips.sui.io',
        },
        'contribute/localize-sui-docs',
        'contribute/code-of-conduct',
        'contribute/style-guide',
      ],
    },
    {
      type: 'category',
      label: 'Run a Node on Sui',
      link: {
        type: 'doc',
        id: 'contribute/nodes/full-node',
      },
      items: [
        'contribute/nodes/full-node',
        'contribute/nodes/validator',
        'contribute/nodes/database-snapshots',
        'contribute/nodes/observability',
      ],
    },
  ]

module.exports = contribute;