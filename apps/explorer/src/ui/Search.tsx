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
	children: React.ReactNode;
}

function SearchItem({ value, children }: SearchResultProps) {
	return (
		<Combobox.Option
			className="cursor-pointer rounded-md py-1.5 pl-2 ui-active:bg-sui/10 ui-active:shadow-sm"
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
}: SearchProps) {
	const hasOptions = !!options.length;
	return (
		<Combobox nullable onChange={onSelectResult} as="div" className="relative w-full">
			<div className="relative flex items-center">
				<Combobox.Input
					spellCheck={false}
					displayValue={(value: SearchResult) => value?.label}
					className="w-full rounded-md border border-transparent bg-search-fill/60 pl-2 text-body leading-9 text-white/20 outline-none placeholder:text-xs placeholder:text-white/40 hover:bg-search-fill hover:placeholder:text-white/60 focus:border-sui focus:bg-search-fill focus:text-white focus:placeholder:text-white/60"
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

				<div className="absolute right-0 mr-2 block items-center text-2xl text-white/20">
					<Search16 />
				</div>
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
								{!!results?.length && (
									<div className="mb-2">
										<Text color="steel-dark" variant="captionSmall/medium">
											{label}
										</Text>
									</div>
								)}
								{results?.map((item) => (
									<SearchItem key={item.id} value={item}>
										{item.label}
									</SearchItem>
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
