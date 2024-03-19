// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { Button } from './ui/button';
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from './ui/dialog';

const WARNING_STATE = 'demo-warning';

export function Warning() {
	const [seenWarning, setSeenWarning] = useState(
		() => localStorage.getItem(WARNING_STATE) === 'true',
	);

	const handleClose = () => {
		setSeenWarning(true);
		localStorage.setItem(WARNING_STATE, 'true');
	};

	return (
		<Dialog open={!seenWarning} onOpenChange={handleClose}>
			<DialogContent className="sm:max-w-[425px]">
				<DialogHeader>
					<DialogTitle>Warning</DialogTitle>
					<DialogDescription>
						This tool is for demonstrative purposes only. It is not intended for production use.
					</DialogDescription>
				</DialogHeader>
				<DialogFooter>
					<Button type="button" onClick={handleClose}>
						Ok
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
