// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Combobox } from '@headlessui/react';
import { Search16 } from '@mysten/icons';

import { LoadingSpinner } from './LoadingSpinner';
import { Text } from './Text';

export type SearchResult = {
    id: string;
    label: string;
    type: string;
};

export type SearchResults = {
    label: string;
    results: SearchResult[];
};

export interface SearchProps {
    onChange: (event: React.ChangeEvent<HTMLInputElement>) => void;
    onSelectResult: (result: SearchResult) => void;
    placeholder?: string;
    isLoading: boolean;
    options?: SearchResults[];
    value?: SearchResult;
    queryValue: string;
}

export interface SearchResultProps {
    key: string;
    value: SearchResult;
    children: React.ReactNode;
}

function SearchItem({ value, children }: SearchResultProps) {
    return (
        <Combobox.Option
            className="cursor-pointer rounded-md bg-opacity-10 py-1.5 pl-2 ui-active:bg-sui/10 ui-active:shadow-sm"
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
    options = [],
    isLoading = false,
    queryValue,
    value,
}: SearchProps) {
    const hasOptions = options.some((group) => group.results.length > 0);
    return (
        <Combobox
            value={value}
            onChange={onSelectResult}
            as="div"
            className="relative flex w-full flex-col"
        >
            <Combobox.Input
                displayValue={(value: SearchResult) => value.label}
                className="border-1 box-border w-full rounded-md border-transparent bg-search-fill/60 pl-2 text-body leading-8 text-white/20 placeholder:text-xs placeholder:text-white/40 focus:border-solid focus:border-sui focus:text-white focus:placeholder:text-white/60"
                onChange={onChange}
                placeholder={placeholder}
                autoComplete="off"
                value={queryValue}
            />

            <Search16 className="absolute right-0 top-0 bg-search-fill text-white opacity-0" />

            {queryValue && (
                <Combobox.Options className="absolute top-9 mt-1 max-h-[500px] w-[500px] list-none overflow-auto rounded-md bg-white p-3.5 shadow-md">
                    {isLoading ? (
                        <div className="flex items-center justify-center">
                            <LoadingSpinner />
                        </div>
                    ) : hasOptions ? (
                        options.map(({ label, results }) => {
                            if (!results.length) return null;
                            return (
                                <div key={label}>
                                    {!!results?.length && (
                                        <div className="mb-2">
                                            <Text
                                                color="steel-dark"
                                                variant="captionSmall/medium"
                                            >
                                                {label}
                                            </Text>
                                        </div>
                                    )}
                                    {results?.map((item) => {
                                        return (
                                            <SearchItem
                                                key={item.id}
                                                value={item}
                                            >
                                                {item.label}
                                            </SearchItem>
                                        );
                                    })}
                                </div>
                            );
                        })
                    ) : (
                        <div className="flex items-center justify-center">
                            <Text
                                variant="body/medium"
                                italic
                                color="steel-darker"
                            >
                                No Results
                            </Text>
                        </div>
                    )}
                </Combobox.Options>
            )}
        </Combobox>
    );
}
