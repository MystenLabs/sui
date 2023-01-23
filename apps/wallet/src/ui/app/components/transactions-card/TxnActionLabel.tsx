// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';

type TxnTypeProps = {
    label: 'Action' | 'From' | 'To';
    address: string;
    actionLabel: string;
};

export function TxnTypeLabel({ label, address, actionLabel }: TxnTypeProps) {
    const content = label !== 'Action' ? address : actionLabel;
    return (
        <div className="flex gap-1 break-all capitalize">
            <Text color="steel-darker" weight="semibold" variant="subtitle">
                {label}:
            </Text>
            <div className="flex-1">
                <Text
                    color="steel-darker"
                    weight="normal"
                    variant="subtitle"
                    mono={label !== 'Action'}
                >
                    {content}
                </Text>
            </div>
        </div>
    );
}
