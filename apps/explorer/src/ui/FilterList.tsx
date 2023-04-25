// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Tab, TabGroup, TabList, type TabGroupProps, type TabListProps } from './Tabs';

export interface FilterListProps<T extends string = string> {
    options: readonly T[];
    value: T;
    disabled?: boolean;
    size?: TabGroupProps['size'];
    lessSpacing?: TabListProps['lessSpacing'];
    onChange(value: T): void;
}

export function FilterList<T extends string>({
    options,
    value,
    disabled = false,
    size,
    lessSpacing,
    onChange,
}: FilterListProps<T>) {
    const selectedIndex = options.indexOf(value);
    return (
        <TabGroup
            size={size}
            selectedIndex={selectedIndex}
            onChange={(index) => {
                onChange(options[index]);
            }}
        >
            <TabList disableBottomBorder lessSpacing={lessSpacing}>
                {options.map((option) => (
                    //@ts-expect-error disabled
                    <Tab disabled={disabled} key={option}>
                        {option}
                    </Tab>
                ))}
            </TabList>
        </TabGroup>
    );
}
