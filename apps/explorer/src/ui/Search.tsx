// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Combobox } from '@headlessui/react';
import { Search16 } from '@mysten/icons';

import { LoadingSpinner } from './LoadingSpinner';
import { Text } from './Text';

type SearchResult = {
    id: number;
    label: string;
};

export interface SearchProps {
    onChange: (event: React.ChangeEvent<HTMLInputElement>) => void;
    onSelectResult: (result: SearchResult) => void;
    placeholder?: string;
    isLoading: boolean;
    options?: Record<string, SearchResult[]>;
    value?: SearchResult;
    inputValue: string;
}

export interface SearchResultProps {
    key: string;
    value: any;
    children: React.ReactNode;
}

function SearchItem({ value, children }: SearchResultProps) {
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
    isLoading = false,
    inputValue,
    value,
}: SearchProps) {
    const hasOptions = Object.entries(options).some(
        ([k, v]) => !!v && Object.keys(v).length
    );
    return (
        <Combobox
            value={value}
            onChange={onSelectResult}
            as="div"
            className="relative flex w-full flex-col"
        >
            <Combobox.Input
                displayValue={(value: SearchResult) => value?.label}
                className="text-white/0.4 border-1 h-[2rem] w-full rounded-md border-transparent bg-search-fill pl-2 text-xs leading-8 text-white focus:border-solid focus:border-sui"
                onChange={onChange}
                placeholder={placeholder}
                autoComplete="off"
                value={inputValue}
            />
            <button
                type="button"
                className="text-white/0.4 absolute inset-y-0 right-0 flex items-center border-none bg-transparent text-2xl focus:outline-none"
            >
                <Search16 className="bg-search-fill text-white opacity-40" />
            </button>

            <Combobox.Options className="absolute top-9 mt-1 max-h-[500px] w-[500px] list-none overflow-auto rounded-md bg-white p-3.5 shadow-md">
                {isLoading ? (
                    <div className="flex items-center justify-center">
                        <LoadingSpinner />
                    </div>
                ) : hasOptions ? (
                    Object.entries(options).map(([key, results], idx) => {
                        if (!results.length) return null;
                        console.log(results);
                        return (
                            <div
                                className={
                                    idx !== Object.entries(options).length - 1
                                        ? 'mb-4'
                                        : ''
                                }
                                key={key}
                            >
                                {!!results?.length && (
                                    <div className="mb-2">
                                        <Text
                                            color="steel-dark"
                                            variant="captionSmall/medium"
                                        >
                                            {key}
                                        </Text>
                                    </div>
                                )}
                                {results?.map((item: any) => {
                                    return (
                                        <SearchItem key={item.id} value={item}>
                                            {item.label}
                                        </SearchItem>
                                    );
                                })}
                            </div>
                        );
                    })
                ) : (
                    <div className="flex items-center justify-center p-5">
                        <Text variant="body/medium" italic color="steel-darker">
                            No Results
                        </Text>
                    </div>
                )}
            </Combobox.Options>
        </Combobox>
    );
}
