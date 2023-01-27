// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Combobox } from '@headlessui/react';
import { useState } from 'react';
import { CheckmarkIcon } from 'react-hot-toast';
import { Text } from './Text';

export interface SearchProps {}

const people = [
    { id: 1, name: 'Durward Reynolds' },
    { id: 2, name: 'Kenton Towne' },
    { id: 3, name: 'Therese Wunsch' },
    { id: 4, name: 'Benedict Kessler' },
    { id: 5, name: 'Katelyn Rohan' },
];

export function Search() {
    const [selectedPerson, setSelectedPerson] = useState(people[0]);
    const [query, setQuery] = useState('');

    const filteredPeople =
        query === ''
            ? people
            : people.filter((person) => {
                  return person.name
                      .toLowerCase()
                      .includes(query.toLowerCase());
              });

    return (
        <Combobox value={selectedPerson} onChange={setSelectedPerson}>
            <Combobox.Input
                className="text-white/0.4 h-[2rem] w-[500px] rounded-md border border-sui bg-search-fill pl-2 font-mono text-xs leading-8 text-white"
                onChange={(event) => setQuery(event.target.value)}
                displayValue={(person) => person.name}
            />
            <Combobox.Options className="text-mono w-[500px] rounded-md pl-0 shadow-md">
                {filteredPeople.map((person) => (
                    <Combobox.Option
                        key={person.id}
                        value={person}
                        className="w-[500px] list-none p-2 ui-active:bg-sui ui-active:text-white ui-not-active:text-black"
                    >
                        <CheckmarkIcon className="hidden ui-selected:block" />
                        <Text variant="captionSmall/medium">{person.name}</Text>
                    </Combobox.Option>
                ))}
            </Combobox.Options>
        </Combobox>
    );
}
