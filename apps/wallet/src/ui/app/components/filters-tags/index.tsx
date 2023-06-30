// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo, useState, useEffect } from 'react';
import ReactDOM from 'react-dom';
import { NavLink } from 'react-router-dom';

import st from './Filters.module.scss';

const ELEMENT_ID = '#sui-apps-filters';

function activeTagsFilter({ isActive }: { isActive: boolean }) {
	return cl({ [st.active]: isActive }, st.filter);
}

// TODO: extend this interface to include params and functions for the filter tags
export interface Props {
	name: string;
	link: string;
}

type Tags = {
	tags: Props[];
	callback?: (tag: Props) => void;
};

function FiltersPortal({ tags, callback }: Tags) {
	const [element, setElement] = useState<HTMLElement | null>(null);

	useEffect(() => {
		const content = document.querySelector(ELEMENT_ID) as HTMLElement;
		if (content) setElement(content);
	}, []);

	return (
		<>
			{element
				? ReactDOM.createPortal(
						<div className={st.filterTags}>
							{tags.map((tag) => {
								return (
									<NavLink
										id={tag.link}
										key={tag.link}
										to={`/${tag.link}`}
										end
										className={activeTagsFilter}
										title={tag.name}
										onClick={callback ? () => callback(tag) : undefined}
									>
										<span className={st.title}>{tag.name}</span>
									</NavLink>
								);
							})}
						</div>,
						element,
				  )
				: null}
		</>
	);
}

export default memo(FiltersPortal);
