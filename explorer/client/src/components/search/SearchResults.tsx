// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';
import { useCallback } from 'react';

import { type ResultType } from './SearchResultType';

import styles from './SearchResults.module.css';

function SearchResults({ result }: { result: ResultType[] | null }) {
    const categoryLabels = {
        objects: 'object',
        transactions: 'transaction',
        addresses: 'address',
    };

    const optionClassName = useCallback((active: boolean) => {
        return active ? styles.selectedoption : styles.notselectedoption;
    }, []);

    if (!result) return <></>;
    return (
        <Combobox.Options as="div" className={styles.results}>
            {result.length === 0 && (
                <p className={styles.noresults}>No Results</p>
            )}

            {result.map((el, index) => (
                <Combobox.Option
                    as="div"
                    key={index}
                    value={el}
                    className={styles.result}
                >
                    {({ active }) => (
                        <dl>
                            <dt>{categoryLabels[el.category]}</dt>
                            <dd className={optionClassName(active)}>
                                {el.input}
                            </dd>
                        </dl>
                    )}
                </Combobox.Option>
            ))}
        </Combobox.Options>
    );
}

export default SearchResults;
