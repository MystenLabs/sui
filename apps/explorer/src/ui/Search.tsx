// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Combobox } from '@headlessui/react';
import { Search16 } from '@mysten/icons';

import { Text } from './Text';

type SearchResult = {
    id: number;
    label: string;
};

export interface SearchProps {
    onChange: (event: React.ChangeEvent<HTMLInputElement>) => void;
    onSelectResult: (result: SearchResult) => void;
    placeholder?: string;
    query: string;
    options?: Record<string, SearchResult[]>;
    value?: SearchResult;
}

export interface SearchResultProps {
    key: string;
    value: any;
    children: React.ReactNode;
}

function SearchResult({ value, children }: SearchResultProps) {
    return (
        <Combobox.Option
            className="cursor-pointer rounded-md bg-opacity-10 py-1.5 pl-2 ui-active:bg-sui ui-active:bg-opacity-10 ui-active:shadow-sm"
            value={value}
            key={value.id}
        >
            <Text variant="body/medium" mono color="steel-darker">
                {children}
            </Text>
        </Combobox.Option>
    );
}

export function Search({
    onChange,
    onSelectResult,
    placeholder,
    options = {},
    value,
}: SearchProps) {
    return (
        <Combobox
            value={value}
            onChange={onSelectResult}
            as="div"
            className="relative flex flex-col"
        >
            <Combobox.Input
                displayValue={(value) => value?.label}
                className="text-white/0.4 border-1 h-[2rem] w-full rounded-md border-transparent bg-search-fill pl-2 text-xs leading-8 text-white focus:border-solid focus:border-sui"
                onChange={onChange}
                placeholder={placeholder}
                autoComplete="off"
            />

            <button
                type="button"
                className="text-white/0.4 absolute inset-y-0 right-0 flex items-center rounded-r-md border-none bg-transparent  text-2xl focus:outline-none"
            >
                <Search16 className="text-white opacity-40" />
            </button>

            <Combobox.Options className="mt-1 w-full list-none rounded-md bg-white p-3.5 shadow-md">
                {Object.entries(options).map(([category, results]) => {
                    return (
                        <div className="mb-4" key={category}>
                            {!!results?.length && (
                                <div className="mb-2">
                                    <Text
                                        color="steel-dark"
                                        variant="captionSmall/medium"
                                    >
                                        {category}
                                    </Text>
                                </div>
                            )}
                            {results?.map((item: any) => {
                                return (
                                    <SearchResult key={item.id} value={item}>
                                        {item.label}
                                    </SearchResult>
                                );
                            })}
                        </div>
                    );
                })}
            </Combobox.Options>
        </Combobox>
    );
}
