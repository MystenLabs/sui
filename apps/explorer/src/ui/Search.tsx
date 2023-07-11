// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Search16 } from '@mysten/icons';
import { LoadingIndicator, Text } from '@mysten/ui';
import { type KeyboardEvent } from 'react';
import { Command } from 'cmdk';
import { useState } from 'react';

export type SearchResult = {
	id: string;
	label: string;
	type: string;
};

export interface SearchProps {
	onChange: (value?: string) => void;
	onSelectResult?: (result: SearchResult) => void;
	placeholder?: string;
	isLoading: boolean;
	options?: SearchResult[];
	queryValue: string;
}

interface SearchResultProps {
	value: SearchResult;
	onSelect: (value: SearchResult) => void;
}

function SearchItem({ value, onSelect }: SearchResultProps) {
	return (
		<Command.Item
			value={`${value.type}/${value.id}`}
			className="group mb-2 cursor-pointer rounded-md px-2 py-1.5 last:mb-0 data-[selected]:bg-sui/10 data-[selected]:shadow-sm"
			onSelect={() => onSelect(value)}
		>
			<div className="flex w-full items-center justify-between">
				<div className="text-body font-medium text-steel-dark group-data-[selected]:text-hero">
					{value.label}
				</div>
				<Text variant="caption/medium" color="steel">
					{value.type}
				</Text>
			</div>
		</Command.Item>
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
	const [visible, setVisible] = useState(false);
	const hasOptions = !!options.length;

	return (
		<Command label="Command Menu" className="relative w-full" shouldFilter={false}>
			<div className="relative flex items-center">
				<div className="absolute left-0 ml-3 block items-center text-2xl text-hero-darkest/80">
					<Search16 />
				</div>

				<Combobox.Input
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
					onValueChange={onChange}
					spellCheck={false}
					className="w-full rounded border border-transparent bg-hero-darkest/5 pl-10 font-mono text-body font-medium leading-9 text-hero-darkest/80 outline-none placeholder:text-sm placeholder:text-hero-darkest/40 hover:bg-hero-darkest/10 focus:bg-hero-darkest/10"
					onFocus={() => setVisible(true)}
					onBlur={() => setVisible(false)}
				/>
			</div>

			{visible && queryValue && (
				<Command.List className="absolute mt-1 w-full list-none rounded-md bg-white p-3.5 shadow-md">
					{isLoading ? (
						<Command.Loading>
							<div className="flex items-center justify-center">
								<LoadingIndicator />
							</div>
						</Command.Loading>
					) : hasOptions ? (
						options.map((item) => (
							<SearchItem
								key={`${item.type}/${item.id}`}
								value={item}
								onSelect={(value) => onSelectResult?.(value)}
							/>
						))
					) : (
						<Command.Item className="flex items-center justify-center" disabled>
							<Text variant="body/medium" italic color="steel-darker">
								No Results
							</Text>
						</Command.Item>
					)}
				</Command.List>
			)}
		</Command>
	);

	// 		{queryValue && (
	// 			<Combobox.Options className="absolute mt-1 w-full list-none space-y-2 rounded-md bg-white p-3.5 shadow-md">
	// 				{isLoading ? (
	// 					<div className="flex items-center justify-center">
	// 						<LoadingSpinner />
	// 					</div>
	// 				) : hasOptions ? (
	// 					options.map(({ label, results }) => (
	// 						<div key={label}>
	// 							{results?.map((item) => (
	// 								<SearchItem key={item.id} value={item} />
	// 							))}
	// 						</div>
	// 					))
	// 			</Combobox.Options>
	// 		)}
	// 	</Combobox>
	// );
}
