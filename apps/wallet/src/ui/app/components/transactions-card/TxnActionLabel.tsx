// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';

type TxnTypeProps = {
    label: 'Action' | 'From' | 'To';
    content: string;
};

export function TxnTypeLabel({ label, content }: TxnTypeProps) {
    return (
        <div className="flex gap-1 break-all capitalize">
            <Text color="steel-darker" weight="semibold" variant="subtitle">
                {label}:
            </Text>
            <Text
                color="steel-darker"
                weight="normal"
                variant="subtitle"
                mono={label !== 'Action'}
            >
                {content}
            </Text>
        </div>
    );
}
