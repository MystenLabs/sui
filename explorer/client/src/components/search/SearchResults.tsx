// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ResultType } from './SearchResultType';
import {Combobox} from '@headlessui/react';

import styles from './SearchResults.module.css';

function SearchResults({
    result,
    resultIndex,
    setResultIndex,
    optionClick,
}: {
    result: ResultType[] | null;
    resultIndex: number;
    setResultIndex: (index: number) => void;
    optionClick: (el: ResultType) => () => void;
}) {
    const categoryLabels = {
        objects: 'object',
        transactions: 'transaction',
        addresses: 'address',
    };

    if (!result) return <></>;
    return (
        <Combobox.Options className={styles.results}>
            {result.length === 0 && (
              <Combobox.Option>
                <p className={styles.noresults}>No Results</p>
              </Combobox.Option>
            )}
            {result.map((el, index) => (
              <Combobox.Option>
                <dl key={index}>
                    <dt>{categoryLabels[el.category]}</dt>
                    <dd
                        className={
                            index === resultIndex ? styles.selectedoption : ''
                        }
                        onClick={optionClick(el)}
                    >
                        {el.input}
                    </dd>
                </dl>
              </Combobox.Option>
            ))}
        </Combobox.Options>
    );
}

export default SearchResults;
