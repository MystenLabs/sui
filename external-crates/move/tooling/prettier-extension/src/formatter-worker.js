// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

'use strict';

const plugin = require('@mysten/prettier-plugin-move');
const { format } = require('prettier');
const { parentPort } = require('node:worker_threads');

/**
 * Upon receiving a message from the parent thread, format the text and send it
 * back. If an error occurs, send the original text back with the error message.
 */
parentPort.on('message', async (message) => {
	const { text, options } = JSON.parse(message);

	return format(text, {
		parser: 'move',
		plugins: [plugin],
		tabWidth: options.tabWidth,
		printWidth: options.printWidth,
		wrapComments: options.wrapComments,
		useModuleLabel: options.useModuleLabel,
		autoGroupImports: options.autoGroupImports,
		enableErrorDebug: options.enableErrorDebug,
	})
		.then((text) => parentPort.postMessage({ text }))
		.catch((err) => parentPort.postMessage({ text, message: err.message }));
});
