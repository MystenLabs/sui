// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Listbox, Transition } from '@headlessui/react';
import { Check12, ChevronDown16 } from '@mysten/icons';
import { Text } from '@mysten/ui';
import { Fragment } from 'react';

export type ListboxSelectPros<T extends string = string> = {
	value: T;
	options: readonly T[];
	onSelect: (value: T) => void;
};

export function ListboxSelect<T extends string>({
	value,
	options,
	onSelect,
}: ListboxSelectPros<T>) {
	return (
		<Listbox value={value} onChange={onSelect}>
			<div className="relative">
				<Listbox.Button className="group flex w-full flex-nowrap items-center gap-1 overflow-hidden text-hero-dark transition-all hover:text-hero-darkest">
					<Text variant="body/semibold">{value}</Text>
					<ChevronDown16
						className="text-gray-400 pointer-events-none h-4 w-4 text-steel transition-all group-hover:text-steel-dark"
						aria-hidden="true"
					/>
				</Listbox.Button>
				<Transition
					as={Fragment}
					leave="transition ease-in duration-100"
					leaveFrom="opacity-100"
					leaveTo="opacity-0"
				>
					<Listbox.Options className="absolute right-0 top-0 z-10 max-h-60 w-max max-w-xs overflow-auto rounded-lg bg-white p-2 shadow">
						{options.map((aValue, index) => (
							<Listbox.Option
								key={index}
								className="flex flex-1 cursor-pointer flex-nowrap items-center gap-4 rounded-sm p-2 hover:bg-sui-light/40"
								value={aValue}
							>
								{({ selected }) => (
									<>
										<span className="flex-1">
											<Text
												variant="caption/medium"
												color={selected ? 'steel-darker' : 'steel-dark'}
												truncate
											>
												{aValue}
											</Text>
										</span>
										{selected ? (
											<Check12 className="h-4 w-4 text-steel-darker" aria-hidden="true" />
										) : null}
									</>
								)}
							</Listbox.Option>
						))}
					</Listbox.Options>
				</Transition>
			</div>
		</Listbox>
	);
}
