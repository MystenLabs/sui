// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import doc from '@doc-data';
import Markdoc from '@markdoc/markdoc';
import React, { useState } from 'react';
import ScrollSpy from 'react-scrollspy-navigation';

import { heading } from '../schema/Heading.markdoc';

interface Heading {
	id: string;
	title: string;
	level: number;
}

/** @type {import('@markdoc/markdoc').Config} */
const config = {
	nodes: {
		heading,
	},
};

function collectHeadings(node: any) {
	let sections: Heading[] = [];
	if (node && node.name === 'article') {
		// Match all h1, h2, h3… tags
		if (node.children.length > 0) {
			for (const n of node.children) {
				if (n.name.match(/h(?!1)\d/)) {
					const title = n.children[0];
					const id = createId(title);
					sections.push({ ...n.attributes, title, id });
				}
			}
		}
	}
	return sections;
}

function createId(title: string) {
	const id = title.toLowerCase().replace(/ /g, '-');
	return id;
}

export default function Help() {
	const ast = Markdoc.parse(doc);
	const content = Markdoc.transform(ast, config);
	const nav: Heading[] = collectHeadings(content);

	const [activeId, setActiveId] = useState('');

	const onChangeActiveId = (current: string, prev: string) => {
		setActiveId(current);
		console.log(current);
	};

	const handleClick = (e) => {
		const { target } = e;
		if (target.tagName === 'A') {
			setActiveId(target.hash.substring(1));
		} else if (target.tagName === 'DIV') {
			window.location.hash = target.querySelector('a').hash;
			setActiveId(target.querySelector('a').hash.substring(1));
		}
	};

	return (
		<div className="flex gap-4">
			<ScrollSpy onChangeActiveId={onChangeActiveId}>
				<nav className="flex-none w-48 text-sm border-2 p-2">
					<div className="sticky top-8">
						<h1>In the help:</h1>
						{nav.length > 0 &&
							nav.map((n) => {
								if (n.id === 'readme') {
									return <></>;
								}
								return (
									<div
										key={n.id}
										className={`nav-item m-2 level-${n.level - 1}${n.id === activeId ? ' nav-active' : ''}`}
										onClick={handleClick}
									>
										<a href={`#${n.id}`}>{n.title}</a>
									</div>
								);
							})}
					</div>
				</nav>
			</ScrollSpy>
			<div className="flex-1 help">{Markdoc.renderers.react(content, React, {})}</div>
		</div>
	);
}
