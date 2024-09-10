// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Listbox, Transition } from '@headlessui/react';
import { KioskOwnerCap } from '@mysten/kiosk';
import { formatAddress } from '@mysten/sui/utils';
import classNames from 'clsx';
import { Fragment } from 'react';

export function KioskSelector({
	caps,
	selected,
	setSelected,
}: {
	caps: KioskOwnerCap[];
	selected: KioskOwnerCap;
	setSelected: (item: KioskOwnerCap) => void;
}) {
	return (
		<div className="max-w-[175px] z-50 relative my-3">
			<label className="font-semibold text-xs">Select a kiosk:</label>
			<Listbox value={selected} onChange={setSelected}>
				<div className="relative mt-1">
					<Listbox.Button
						className="relative w-full rounded-lg
           bg-white py-2 pl-3 pr-10 text-left focus:outline-none
           border border-primary
           focus-visible:border-primary focus-visible:ring-2
           focus-visible:ring-white focus-visible:ring-opacity-75 focus-visible:ring-offset-2
            focus-visible:ring-offset-primary sm:text-sm z-50
            cursor-pointer
            "
					>
						<span className="block truncate">{formatAddress(selected.kioskId)}</span>
						<span className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2">
							{/* <ChevronUpDownIcon
                className="h-5 w-5 text-gray-400"
                aria-hidden="true"
              /> */}
						</span>
					</Listbox.Button>
					<Transition
						as={Fragment}
						leave="transition ease-in duration-100"
						leaveFrom="opacity-100"
						leaveTo="opacity-0"
					>
						<Listbox.Options
							className="absolute mt-1 max-h-60 w-full overflow-y-auto overflow-x-hidden rounded-md
             bg-white text-base shadow-lg ring-1
             border border-primary
             ring-black ring-opacity-5 focus:outline-none sm:text-sm"
						>
							{caps.map((cap) => (
								<Listbox.Option
									key={cap.objectId}
									className={({ active, selected }) =>
										classNames(
											selected || active ? 'bg-primary text-white' : 'text-primary',
											'relative select-none cursor-pointer py-2 my-1 px-4',
										)
									}
									value={cap}
								>
									{({ selected }) => (
										<>
											<span
												className={`block truncate ${selected ? 'font-medium' : 'font-normal'}`}
											>
												{formatAddress(cap.kioskId)}
											</span>
											{selected ? (
												<span className="absolute inset-y-0 left-0 flex items-center text-primary">
													{/* <CheckIcon className="h-5 w-5" aria-hidden="true" /> */}
												</span>
											) : null}
										</>
									)}
								</Listbox.Option>
							))}
						</Listbox.Options>
					</Transition>
				</div>
			</Listbox>
		</div>
	);
}
