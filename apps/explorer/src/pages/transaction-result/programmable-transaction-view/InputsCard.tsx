// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiCallArg } from '@mysten/sui.js';

import {
    ExpandableList,
    ExpandableListControl,
    ExpandableListItems,
} from '~/ui/ExpandableList';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import {
    TransactionBlockCard,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';

const REGEX_NUMBER = /^\d+$/;
const DEFAULT_ITEMS_TO_SHOW = 5;

interface InputsCardProps {
    inputs: SuiCallArg[];
}

export function InputsCard({ inputs }: InputsCardProps) {
    const defaultOpen = inputs.length <= DEFAULT_ITEMS_TO_SHOW;

    if (!inputs?.length) {
        return null;
    }

    const expandableItems = inputs.map((input, index) => (
        <TransactionBlockCardSection
            key={index}
            title={`Input ${index}`}
            defaultOpen={defaultOpen}
        >
            <div className="flex flex-col gap-3">
                {Object.entries(input).map(([key, value]) => {
                    let renderValue;
                    const stringValue = String(value);

                    if (key === 'mutable') {
                        renderValue = String(value);
                    } else if (key === 'objectId') {
                        renderValue = <ObjectLink objectId={stringValue} />;
                    } else if (
                        'valueType' in input &&
                        'value' in input &&
                        input.valueType === 'address' &&
                        key === 'value'
                    ) {
                        renderValue = <AddressLink address={stringValue} />;
                    } else if (REGEX_NUMBER.test(stringValue)) {
                        renderValue = Number(value).toLocaleString();
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
        </TransactionBlockCardSection>
    ));

    return (
        <TransactionBlockCard collapsible title="Inputs">
            <ExpandableList
                items={expandableItems}
                defaultItemsToShow={DEFAULT_ITEMS_TO_SHOW}
                itemsLabel="Inputs"
            >
                <div className="flex max-h-[300px] flex-col gap-6 overflow-y-auto">
                    <ExpandableListItems />
                </div>

                {expandableItems.length > DEFAULT_ITEMS_TO_SHOW && (
                    <div className="mt-6">
                        <ExpandableListControl />
                    </div>
                )}
            </ExpandableList>
        </TransactionBlockCard>
    );
}
