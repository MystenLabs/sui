// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';

import { type ResultType } from './SearchResultType';

import styles from './SearchResults.module.css';

function SearchResults({ result }: { result: ResultType[] | null }) {
    const categoryLabels = {
        objects: 'object',
        transactions: 'transaction',
        addresses: 'address',
    };

    if (!result) return <></>;
    return (
        <Combobox.Options as="div" className={styles.results}>

            {result.length === 0 && (
                <p className={styles.noresults}>
                    No Results
                </p>
            )}

            {result.map((el, index) => (
                <Combobox.Option as="dl" 
                 key={index} value={el.input}
                 className={({active}) => 
                   `${styles.result} ${
                     active ? styles.selectedoption : ''
                   }`
                 }
            >
                        <dt>{categoryLabels[el.category]}</dt>
                        <dd>{el.input}</dd>
                </Combobox.Option>
            ))}

        </Combobox.Options>
    );
}

export default SearchResults;
