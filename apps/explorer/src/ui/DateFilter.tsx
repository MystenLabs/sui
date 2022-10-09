// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { Tab, TabGroup, TabList } from './Tabs';

export type DateFilterOption = 'D' | 'W' | 'M' | 'ALL';

export function useDateFilterState(defaultFilter: DateFilterOption) {
    return useState(defaultFilter);
}

export interface DateFilterProps {
    options?: DateFilterOption[];
    value: DateFilterOption;
    onChange(value: DateFilterOption): void;
}

export function DateFilter({
    options = ['D', 'W', 'M', 'ALL'],
    value,
    onChange,
}: DateFilterProps) {
    const selectedIndex = options.indexOf(value);

    return (
        <TabGroup
            selectedIndex={selectedIndex}
            onChange={(index) => {
                onChange(options[index]);
            }}
        >
            <TabList disableBottomBorder>
                {options.map((option) => (
                    <Tab key={option}>{option}</Tab>
                ))}
            </TabList>
        </TabGroup>
    );
}
