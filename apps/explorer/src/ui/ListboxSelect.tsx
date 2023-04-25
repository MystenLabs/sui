// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Listbox, Transition } from '@headlessui/react';
import { Check12, ChevronDown16 } from '@mysten/icons';
import { Fragment } from 'react';

import { Text } from './Text';

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
            {({ open }) => (
                <div className="relative">
                    <Listbox.Button className="flex w-full flex-nowrap items-center gap-1 overflow-hidden rounded-lg border border-solid p-2 transition-all ui-open:border-steel ui-not-open:border-transparent ui-not-open:hover:border-steel">
                        <Text
                            variant="captionSmall/normal"
                            color="steel-darker"
                        >
                            {value}
                        </Text>
                        <ChevronDown16
                            className="text-gray-400 pointer-events-none h-4 w-4 text-steel"
                            aria-hidden="true"
                        />
                    </Listbox.Button>
                    <Transition
                        as={Fragment}
                        leave="transition ease-in duration-100"
                        leaveFrom="opacity-100"
                        leaveTo="opacity-0"
                    >
                        <Listbox.Options className="absolute right-2.5 top-2.5 z-10 max-h-60 w-max max-w-xs overflow-auto rounded-lg bg-white p-2 shadow">
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
                                                    color={
                                                        selected
                                                            ? 'steel-darker'
                                                            : 'steel-dark'
                                                    }
                                                    truncate
                                                >
                                                    {aValue}
                                                </Text>
                                            </span>
                                            {selected ? (
                                                <Check12
                                                    className="h-4 w-4 text-success"
                                                    aria-hidden="true"
                                                />
                                            ) : null}
                                        </>
                                    )}
                                </Listbox.Option>
                            ))}
                        </Listbox.Options>
                    </Transition>
                </div>
            )}
        </Listbox>
    );
}
