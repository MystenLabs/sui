// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '~/ui/Disclosure';
import { ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';

type Version = {
    number: number;
    id: string;
};

// todo: mark as required and remove props when we have real data
interface VersionsDisclosureProps {
    versions?: Version[];
}

const mockVersions = [
    {
        number: 5,
        id: '0x0000000000000000000000000000000000000000000000000000000000000006',
    },
    {
        number: 4,
        id: '0x0000000000000000000000000000000000000000000000000000000000000005',
    },
    {
        number: 3,
        id: '0x0000000000000000000000000000000000000000000000000000000000000004',
    },
    {
        number: 2,
        id: '0x0000000000000000000000000000000000000000000000000000000000000003',
    },
    {
        number: 1,
        id: '0x0000000000000000000000000000000000000000000000000000000000000002',
    },
];

export function VersionsDisclosure({
    versions = mockVersions,
}: VersionsDisclosureProps) {
    return (
        <Disclosure
            variant="inline"
            title={`${versions.length} ${
                versions.length > 1 ? 'versions' : 'version'
            }`}
        >
            <div className="flex flex-col gap-2">
                {versions.map((version, idx) => (
                    <div key={version.id} className="flex items-center gap-3">
                        <Text
                            variant="p1/semibold"
                            mono
                            color={idx === 0 ? 'steel-darker' : 'steel'}
                        >
                            v{version.number}
                        </Text>
                        {idx === 0 ? (
                            <div className="break-all">
                                <Text
                                    mono
                                    variant="bodySmall/medium"
                                    color="steel-darker"
                                >
                                    {version.id}
                                </Text>
                            </div>
                        ) : (
                            <ObjectLink noTruncate objectId={version.id} />
                        )}
                    </div>
                ))}
            </div>
        </Disclosure>
    );
}
