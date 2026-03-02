// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const path = require('path');
const vscode = require('vscode');
const prettier = require('prettier');
const { Worker } = require('node:worker_threads');

/**
 * Extension name must match the name in `package.json`, as it is the way to
 * read the configuration settings.
 */
const EXTENSION_NAME = 'prettierMove';

/**
 * Max time to wait for the worker to respond.
 */
const WAIT_TIME = 5000;

/**
 * Stores the worker instance.
 */
let worker;

/**
 * Output channel to display messages.
 */
const channel = vscode.window.createOutputChannel('Prettier Move');

/**
 * Start the worker and register the document range formatting provider.
 * Upon activation, the extension reads the configuration settings in the following order:
 *
 * - .prettierrc
 * - Extension settings
 * - Prettier extension settings
 */
function activate(context) {
	worker = new Worker(path.join(__dirname, 'formatter-worker.js'));
	const langs = [
		{ scheme: 'file', language: 'move' },
		{ scheme: 'untitled', language: 'move' },
	];

	context.subscriptions.push(
		vscode.languages.registerDocumentRangeFormattingEditProvider(langs, {
			provideDocumentRangeFormattingEdits: async (document, range, _opts, token) => {
				const options = await findMatchingConfig(document.uri);

				channel.appendLine('Sending text to worker for: ' + document.uri.fsPath);

				// send the text and options to the worker
				worker.postMessage(
					JSON.stringify({
						text: document.getText(),
						options,
						documentUri: document.uri.fsPath,
					}),
				);

				// wait for the worker to send the formatted text back. If it
				// takes longer than 5 seconds, reject the promise.
				const { text: edited, documentUri } = await new Promise((resolve, reject) => {
					setTimeout(() => {
						reject();
						worker.off('message', handleMessage);
					}, WAIT_TIME);

					worker.on('message', handleMessage);

					function handleMessage({ text, message, documentUri, error }) {
						if (error) {
							channel.appendLine('Error formatting text: ' + message);
							reject(new Error(message));
							worker.off('message', handleMessage);
							return;
						}

						if (documentUri === document.uri.fsPath) {
							channel.appendLine('Received formatted text for: ' + documentUri);
							resolve({ text, documentUri });
							worker.off('message', handleMessage);
						} else {
							channel.appendLine('Message from wrong document: ' + documentUri);
						}
					}
				});

				channel.appendLine('Document: ' + document.uri.fsPath);

				return [vscode.TextEdit.replace(range, edited)];
			},
		}),
	);
}

/**
 * For the given filepath, search for a prettier configuration file and resolve
 * it with overrides applied for the specific file path.
 *
 * Falls back to extension settings if no config file is found.
 */
async function findMatchingConfig(documentUri) {
	const now = Date.now();
	const filePath = documentUri.fsPath;

	// Clear cosmiconfig's internal search cache so that config file
	// changes are picked up without restarting the extension.
	await prettier.clearConfigCache();

	// prettier.resolveConfig uses cosmiconfig internally and resolves
	// `overrides` by matching glob patterns against the file path.
	const resolved = await prettier.resolveConfig(filePath, {
		editorconfig: true,
		useCache: false,
	});

	const formatterConfig = vscode.workspace.getConfiguration(EXTENSION_NAME);

	if (resolved) {
		channel.appendLine(`Resolved prettier config for ${filePath}`);

		const config = {
			wrapComments: formatterConfig.get('wrapComments'),
			useModuleLabel: formatterConfig.get('useModuleLabel'),
			autoGroupImports: formatterConfig.get('autoGroupImports'),
			enableErrorDebug: formatterConfig.get('errorDebugMode'),
			...resolved,
		};

		channel.appendLine('Resulting config:');
		channel.append(JSON.stringify(config, null, 2));
		channel.appendLine(`Time taken to resolve config: ${Date.now() - now}ms`);

		return config;
	}

	channel.appendLine(`No prettier config found for ${filePath}, using extension settings`);

	return {
		tabWidth: formatterConfig.get('tabWidth'),
		printWidth: formatterConfig.get('printWidth'),
		wrapComments: formatterConfig.get('wrapComments'),
		useModuleLabel: formatterConfig.get('useModuleLabel'),
		autoGroupImports: formatterConfig.get('autoGroupImports'),
		enableErrorDebug: formatterConfig.get('errorDebugMode'),
	};
}

/**
 * Deactivate the extension by terminating the worker.
 */
function deactivate() {
	worker && worker.terminate();
	channel.dispose();
}

module.exports = {
	activate,
	deactivate,
};
