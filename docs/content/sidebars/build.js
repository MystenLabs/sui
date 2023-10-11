const build = [
    {
      type: 'doc',
      id: 'build',
      label: 'Build',
    },
    {
      type: 'category',
      label: 'Quickstart',
      link: {
        type: 'doc',
        id: 'build/quickstart/about',
      },
      items: [
        'build/quickstart/about',
        'build/quickstart/examples',
        'build/quickstart/tutorials',
        ],
    },
    {
      type: 'category',
      label: 'Environment Setup',
      link: {
        type: 'doc',
        id: 'build/setup/connect-to-a-network',
      },
      items: [
        'build/setup/connect-to-a-network',
        'build/setup/faucet',
        'build/setup/local-network',
        'build/setup/gas-changes',
        'build/setup/using-the-api',
        {
          type: 'category',
          label: 'Setup the CLI',
          link: {
            type: 'doc',
            id: 'build/setup/cli/install-sui',
          },
          items: [
            'build/setup/cli/install-sui',
            'build/setup/cli/client-cli',
          ],
        },
      ],
    },
    {
      type: 'category',
      label: 'Smart Contracts with Move',
      link: {
        type: 'doc',
        id: 'build/create-smart-contracts/smart-contracts',
      },
      items: [
        'build/create-smart-contracts/smart-contracts',
        'build/create-smart-contracts/write-move-packages',
        'build/create-smart-contracts/build-and-test',
        'build/create-smart-contracts/debug-and-publish',
        'build/create-smart-contracts/move-toml',
        'build/create-smart-contracts/move-lock',
        'build/create-smart-contracts/time',
        'build/create-smart-contracts/upgrade-packages',
        'build/create-smart-contracts/custom-upgrade-policies',
        'build/create-smart-contracts/dependency-overrides',
        'build/create-smart-contracts/sui-move-library',
      ],
    },
    {
      type: 'category',
      label: 'Program With Objects',
      link: {
        type: 'doc',
        id: 'build/program-with-objects/object-basics',
      },
      items: [
        'build/program-with-objects/object-basics',
        'build/program-with-objects/using-objects',
        'build/program-with-objects/immutable-objects',
        'build/program-with-objects/object-wrapping',
        'build/program-with-objects/dynamic-fields',
        'build/program-with-objects/collections',
        'build/program-with-objects/object-display-standard',
      ],
    },
    'build/programmable-tx-blocks',
  ]

  module.exports = build;