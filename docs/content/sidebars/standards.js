const standards = [
    'standards',
    'standards/kiosk',
    {
        type: 'category',
        label: 'DeepBook',
        link: {
            type: 'doc',
            id: 'standards/deepbook'
        },
        items: [
            'standards/deepbook/design',
            'standards/deepbook/orders',
            'standards/deepbook/pools',
            'standards/deepbook/query-the-pool',
            'standards/deepbook/routing-a-swap',
            'standards/deepbook/trade-and-swap',
        ]
    },
    'standards/display',
    'standards/wallet-adapter',
]
module.exports = standards;