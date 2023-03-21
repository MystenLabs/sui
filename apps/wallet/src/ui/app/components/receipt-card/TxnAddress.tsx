// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TxnAddressLink } from './TxnAddressLink';
import { Text } from '_src/ui/app/shared/text';

type TxnAddressProps = {
    address: string;
    label: string;
};

export function TxnAddress({ address, label }: TxnAddressProps) {
    return (
        <div className="flex justify-between w-full items-center py-3.5 first:pt-0">
            <Text variant="body" weight="medium" color="steel-darker">
                {label}
            </Text>
            <div className="flex gap-1 items-center">
                <TxnAddressLink address={address} />
            </div>
        </div>
    );
}
