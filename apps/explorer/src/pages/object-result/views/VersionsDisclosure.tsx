// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { DisclosureBox } from '~/ui/DisclosureBox';
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
        number: 1,
        id: '0x0000000000000000000000000000000000000000000000000000000000000002',
    },
    {
        number: 2,
        id: '0x0000000000000000000000000000000000000000000000000000000000000003',
    },
    {
        number: 3,
        id: '0x0000000000000000000000000000000000000000000000000000000000000004',
    },
    {
        number: 4,
        id: '0x0000000000000000000000000000000000000000000000000000000000000005',
    },
    {
        number: 5,
        id: '0x0000000000000000000000000000000000000000000000000000000000000006',
    },
];

export function VersionsDisclosure({
    versions = mockVersions,
}: VersionsDisclosureProps) {
    const sorted = [...versions].reverse();
    return (
        <DisclosureBox
            variant="inline"
            title={`${versions.length} version${
                versions.length > 1 ? 's' : ''
            }`}
        >
            <div className="flex flex-col gap-2">
                {sorted.map((version, idx) => (
                    <div
                        key={version.number}
                        className="flex items-center gap-3"
                    >
                        <Text
                            variant="p1/semibold"
                            mono
                            color={idx === 0 ? 'steel-darker' : 'steel'}
                        >
                            v{version.number}
                        </Text>
                        {idx === 0 ? (
                            <Text
                                mono
                                variant="bodySmall/medium"
                                color="steel-darker"
                            >
                                {version.id}
                            </Text>
                        ) : (
                            <ObjectLink noTruncate objectId={version.id} />
                        )}
                    </div>
                ))}
            </div>
        </DisclosureBox>
    );
}
