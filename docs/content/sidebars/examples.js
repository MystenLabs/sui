const examples = [
    { 
      type: 'doc',
      id: 'examples',
      label: 'Examples',
    },
    {
      type: 'category',
      label: 'Sui Move Basics',
      link: {
        type: 'doc',
        id: 'sui-examples/movetoml',
      },
      items: [
        'sui-examples/movetoml',
        'sui-examples/init',
        'sui-examples/entry-functions',
        'sui-examples/strings',
        'sui-examples/shared-objects',
        'sui-examples/transferring-objects',
        'sui-examples/custom-transfer',
        'sui-examples/events',
        'sui-examples/otw',
        'sui-examples/publisher',
        'sui-examples/object-display',
      ],
    },
    {
      type: 'category',
      label: 'Patterns',
      link: {
        type: 'doc',
        id: 'sui-examples/capability',
      },
      items: [
        'sui-examples/capability',
        'sui-examples/witness',
        'sui-examples/transferrable-witness',
        'sui-examples/hot-potato',
        'sui-examples/id-pointer',
      ],
    },
    {
      type: 'category',
      label: 'Samples',
      link: {
        type: 'doc',
        id: 'sui-examples/create-an-nft',
      },
      items: [
        'sui-examples/create-an-nft',
        'sui-examples/create-a-coin',
      ],
    },
    'sui-examples/additional-resources',
  ]


module.exports = examples;