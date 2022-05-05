export async function mockGetObjectInfo(object_ids: string[]) {
    return [
        {
            status: 'Exists',
            details: {
                objectRef: {
                    objectId: 'dbe11ea56da16992d8e68d07427fe74a85606d5e',
                    version: 2,
                    digest: 'hobnw2DdC2quOLZf7D7QiRtAJGPP3GeAJA3iPmEoEaA=',
                },
                objectType: 'moveObject',
                object: {
                    contents: {
                        fields: {
                            balance: {
                                fields: {
                                    value: 50000,
                                },
                                type: '0x2::Balance::Balance<0x2::SUI::SUI>',
                            },
                            id: {
                                fields: {
                                    id: {
                                        fields: {
                                            id: {
                                                fields: {
                                                    bytes: 'dbe11ea56da16992d8e68d07427fe74a85606d5e',
                                                },
                                                type: '0x2::ID::ID',
                                            },
                                        },
                                        type: '0x2::ID::UniqueID',
                                    },
                                    version: 2,
                                },
                                type: '0x2::ID::VersionedID',
                            },
                        },
                        type: '0x2::Coin::Coin<0x2::SUI::SUI>',
                    },
                    owner: {
                        AddressOwner:
                            'f16a5aedcdf9f2a9c2bd0f077279ec3d5ff0dfee',
                    },
                    tx_digest: 'vU/KG88bjX8VgdNtflBjNUFNZAqYv2qMCwP9S0+tQRc=',
                },
            },
        },
    ];
}
