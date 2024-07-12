// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ExclamationTriangleIcon } from '@radix-ui/react-icons';
import { Callout, Container, Strong } from '@radix-ui/themes';
import { ReactElement, ReactNode } from 'react';

interface Props {
	title: string;
	children: ReactNode;
}

/**
 * Component for displaying an error message as a call-out. The
 * `title` is displayed as a heading and the `children` are displayed
 * underneath.
 */
export function Error({ title, children }: Props): ReactElement {
	return (
		<Container size="1" px="2">
			<Callout.Root color="red" role="alert" size="3" variant="surface">
				<Callout.Icon>
					<ExclamationTriangleIcon />
				</Callout.Icon>
				<Callout.Text>
					<Strong>{title}</Strong>
					<br />
					{children}
				</Callout.Text>
			</Callout.Root>
		</Container>
	);
}
