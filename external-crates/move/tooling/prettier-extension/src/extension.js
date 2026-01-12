// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const path = require('path');
const vscode = require('vscode');
const prettier = require('prettier');
const { cosmiconfigSync: cosmiconfig } = require('cosmiconfig');
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
						channel.appendLine('Timeout waiting for formatted text for: ' + documentUri);
					}, WAIT_TIME);

					worker.on('message', handleMessage);

					function handleMessage({ text, message, documentUri }) {
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
 * For the given filepath, search for one of the following configuration files:
 * - .prettierrc (prettier.json etc)
 *
 * Alternatively use (in order, if set):
 * - Extension settings
 * - Prettier extension settings
 */
async function findMatchingConfig(documentUri) {
	const workspaceFolder = vscode.workspace.getWorkspaceFolder(documentUri);
	if (!workspaceFolder) {
		const formatterConfig = vscode.workspace.getConfiguration(EXTENSION_NAME);
		return {
			tabWidth: formatterConfig.get('tabWidth'),
			printWidth: formatterConfig.get('printWidth'),
			wrapComments: formatterConfig.get('wrapComments'),
			useModuleLabel: formatterConfig.get('useModuleLabel'),
			autoGroupImports: formatterConfig.get('autoGroupImports'),
			enableErrorDebug: formatterConfig.get('errorDebugMode'),
		};
	}

	const root = workspaceFolder.uri.fsPath;
	let lookup = documentUri.fsPath;
	let search = {};

	// go back in the directory until the root is found; or until we find the
	// .prettierrc (.json | .yml) file
	while (lookup !== root && lookup !== '/') {
		lookup = path.join(lookup, '..');

		const prettierConfig = cosmiconfig('prettier', {
			searchPlaces: [
				'.prettierrc',
				'.prettierrc.json',
				'.prettierrc.yaml',
				'.prettierrc.yml',
				'.prettierrc.js',
				'prettier.config.js',
			],
		}).search(lookup);

		if (prettierConfig) {
			channel.appendLine(`Found a prettier config at ${prettierConfig.filepath}`);
			search = prettierConfig.config;
			channel.append(JSON.stringify(search, null, 2));
			break;
		}
	}

	const formatterConfig = vscode.workspace.getConfiguration(EXTENSION_NAME);

	return {
		tabWidth: formatterConfig.get('tabWidth'),
		printWidth: formatterConfig.get('printWidth'),
		wrapComments: formatterConfig.get('wrapComments'),
		useModuleLabel: formatterConfig.get('useModuleLabel'),
		autoGroupImports: formatterConfig.get('autoGroupImports'),
		enableErrorDebug: formatterConfig.get('errorDebugMode'),
		...search, // .prettierrc overrides the extension settings
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
