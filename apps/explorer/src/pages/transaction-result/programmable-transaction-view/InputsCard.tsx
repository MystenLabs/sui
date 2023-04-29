// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiCallArg } from '@mysten/sui.js';

import { ExpandableList } from '~/ui/ExpandableList';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import { TransactionCard, TransactionCardSection } from '~/ui/TransactionCard';

interface InputsCardProps {
    inputs: SuiCallArg[];
}

export function InputsCard({ inputs }: InputsCardProps) {
    const collapsedThreshold = inputs.length > 5;
    const defaultItemsToShow = collapsedThreshold ? 5 : inputs?.length;
    const regNumber = /^\d+$/;

    if (!inputs?.length) {
        return null;
    }

    const expandableItems = inputs.map((input, index) => {
        const inputTitle = `Input ${index}`;
        const inputEntries = Object.entries(input);

        return (
            <TransactionCardSection
                key={inputTitle}
                title={inputTitle}
                collapsedOnLoad={collapsedThreshold}
            >
                <div className="flex flex-col gap-3">
                    {inputEntries.map(([key, value]) => {
                        let renderValue;
                        const stringValue = String(value);

                        if (key === 'mutable') {
                            renderValue = String(value);
                        } else if (regNumber.test(stringValue)) {
                            renderValue = Number(value).toLocaleString();
                        } else if (key === 'objectId') {
                            renderValue = <ObjectLink objectId={stringValue} />;
                        } else if (
                            'valueType' in input &&
                            'value' in input &&
                            input.valueType === 'address' &&
                            key === 'value'
                        ) {
                            renderValue = <AddressLink address={stringValue} />;
                        } else {
                            renderValue = stringValue;
                        }

                        return (
                            <div
                                key={key}
                                className="flex items-center justify-between gap-12"
                            >
                                <Text variant="pBody/medium" color="steel-dark">
                                    {key}
                                </Text>

                                <Text
                                    truncate
                                    variant="pBody/medium"
                                    color="steel-darker"
                                >
                                    <div className="truncate capitalize">
                                        {renderValue}
                                    </div>
                                </Text>
                            </div>
                        );
                    })}
                </div>
            </TransactionCardSection>
        );
    });

    return (
        <TransactionCard collapsible title="Inputs">
            <ExpandableList
                items={expandableItems}
                defaultItemsToShow={defaultItemsToShow}
                itemsLabel="Inputs"
            />
        </TransactionCard>
    );
}
