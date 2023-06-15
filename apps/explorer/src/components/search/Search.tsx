// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import React, { useState, useCallback } from 'react';

import { useDebouncedValue } from '~/hooks/useDebouncedValue';
import { useSearch } from '~/hooks/useSearch';
import { Search as SearchBox, type SearchResult } from '~/ui/Search';
import { useNavigateWithQuery } from '~/ui/utils/LinkWithQuery';

function Search() {
	const [query, setQuery] = useState('');
	const debouncedQuery = useDebouncedValue(query);
	const { isLoading, data: results } = useSearch(debouncedQuery);
	const handleTextChange = useCallback(
		(e: React.ChangeEvent<HTMLInputElement>) => setQuery(e.currentTarget.value.trim()),
		[setQuery],
	);
	const navigate = useNavigateWithQuery();
	const handleSelectResult = useCallback(
		(result: SearchResult) => {
			if (result) {
				navigate(`/${result?.type}/${encodeURIComponent(result?.id)}`, {});
				setQuery('');
			}
		},
		[navigate],
	);

	return (
		<div className="flex max-w-lg">
			<SearchBox
				queryValue={query}
				onChange={handleTextChange}
				onSelectResult={handleSelectResult}
				placeholder="Search Addresses / Objects / Transactions"
				isLoading={isLoading}
				options={results}
			/>
		</div>
	);
}

export default Search;
