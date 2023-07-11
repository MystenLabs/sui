// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Combobox } from '@headlessui/react';
import { Search16 } from '@mysten/icons';
import { type KeyboardEvent } from 'react';

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
	onSelectResult?: (result: SearchResult) => void;
	placeholder?: string;
	isLoading: boolean;
	options?: SearchResults[];
	queryValue: string;
}

export interface SearchResultProps {
	key: string;
	value: SearchResult;
}

function SearchItem({ value }: SearchResultProps) {
	return (
		<Combobox.Option
			className="cursor-pointer rounded-md px-2 py-1.5 ui-active:bg-sui/10 ui-active:shadow-sm"
			value={value}
			key={value.id}
		>
			<div className="flex w-full items-center justify-between">
				<div className="text-body font-medium text-steel-dark ui-active:text-hero">
					{value.label}
				</div>
				<Text variant="caption/medium" color="steel">
					{value.type}
				</Text>
			</div>
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
}: SearchProps) {
	const hasOptions = !!options.length;
	return (
		<Combobox nullable onChange={onSelectResult} as="div" className="relative w-full">
			<div className="relative flex items-center">
				<div className="absolute left-0 ml-3 block items-center text-2xl text-white/20">
					<Search16 />
				</div>

				<Combobox.Input
					spellCheck={false}
					displayValue={(value: SearchResult) => value?.label}
					className="w-full rounded-md border border-transparent bg-search-fill/60 pl-10 text-body leading-9 text-white/20 outline-none placeholder:text-xs placeholder:text-white/40 hover:bg-search-fill hover:placeholder:text-white/60 focus:border-sui focus:bg-search-fill focus:text-white focus:placeholder:text-white/60"
					onChange={onChange}
					placeholder={placeholder}
					autoComplete="off"
					onKeyDown={(e: KeyboardEvent<HTMLInputElement>) => {
						if (e.code === 'Enter' && !hasOptions) {
							e.stopPropagation();
							e.preventDefault();
						}
					}}
					value={queryValue}
				/>
			</div>

			{queryValue && (
				<Combobox.Options className="absolute mt-1 w-full list-none space-y-2 rounded-md bg-white p-3.5 shadow-md">
					{isLoading ? (
						<div className="flex items-center justify-center">
							<LoadingSpinner />
						</div>
					) : hasOptions ? (
						options.map(({ label, results }) => (
							<div key={label}>
								{results?.map((item) => (
									<SearchItem key={item.id} value={item} />
								))}
							</div>
						))
					) : (
						<div className="flex items-center justify-center">
							<Text variant="body/medium" italic color="steel-darker">
								No Results
							</Text>
						</div>
					)}
				</Combobox.Options>
			)}
		</Combobox>
	);
}
