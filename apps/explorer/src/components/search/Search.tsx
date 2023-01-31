// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React, { useState, useCallback } from 'react';

import { useDebouncedValue } from '~/hooks/useDebounce';
import { useSearch } from '~/hooks/useSearch';
import { Search as SearchBox } from '~/ui/Search';
import { useNavigateWithQuery } from '~/ui/utils/LinkWithQuery';

function Search() {
    const [input, setInput] = useState('');
    const query = useDebouncedValue(input);
    const { isLoading, data: results } = useSearch(query);
    const handleTextChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
            setInput(e.currentTarget.value.trim()),
        [setInput]
    );
    const navigate = useNavigateWithQuery();
    const handleSelectResult = useCallback(
        (result: any) => {
            navigate(`/${result.type}/${encodeURIComponent(result.id)}`, {});
        },
        [navigate]
    );
    return (
        <div className="flex max-w-lg">
            <SearchBox
                inputValue={input}
                onChange={handleTextChange}
                onSelectResult={handleSelectResult}
                placeholder="Search Addresses / Objects / Transactions / Epochs"
                isLoading={isLoading}
                options={results}
            />
        </div>
    );
}

export default Search;
