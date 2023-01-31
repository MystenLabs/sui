// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState, useMemo } from 'react';

import { Search, type SearchProps } from '../Search';

export default {
    component: Search,
} as Meta;

const options = {
    transaction: [
        { id: 1, label: 'transaction 1' },
        { id: 2, label: 'transaction 2' },
        { id: 3, label: 'transaction 3' },
        { id: 4, label: 'transaction 4' },
    ],
    object: [
        { id: 1, label: 'object 1' },
        { id: 2, label: 'object 2' },
        { id: 3, label: 'object 3' },
        { id: 4, label: 'object 4' },
    ],
    address: [
        { id: 1, label: 'address 1' },
        { id: 2, label: 'address 2' },
        { id: 3, label: 'address 3' },
        { id: 4, label: 'address 4' },
    ],
};

export const Default: StoryObj<SearchProps> = {
    args: {},
    render: () => {
        const [query, setQuery] = useState('');
        const [value, setValue] = useState(undefined);
        const filteredOptions = useMemo(() => {
            const filtered = Object.entries(options).reduce(
                (acc, [key, value]) => {
                    const filtered = value.filter((option) =>
                        option.label.toLowerCase().includes(query.toLowerCase())
                    );
                    if (filtered.length) {
                        acc[key] = filtered;
                    }
                    return acc;
                },
                {} as any
            );
            return filtered;
        }, [query]);

        return (
            <div className="flex h-screen w-screen bg-headerNav p-10">
                <div className="w-[500px] ">
                    <Search
                        inputValue={query}
                        isLoading={false}
                        onChange={(e) => setQuery(e.currentTarget.value)}
                        placeholder="Search Addresses / Objects / Transactions / Epochs"
                        onSelectResult={(val) => {
                            setValue(value);
                        }}
                        value={value}
                        options={filteredOptions}
                    />
                </div>
            </div>
        );
    },
};
