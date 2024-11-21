// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const path = require('path');
const vscode = require('vscode');
const { cosmiconfigSync: cosmiconfig } = require('cosmiconfig');
const { Worker } = require('node:worker_threads');

/**
 * Extension name must match the name in `package.json`, as it is the way to
 * read the configuration settings.
 */
const EXTENSION_NAME = 'prettierMove';

/**
 * Prettier extension name.
 * If installed, this extension settings will be used.
 */
const PRETTIER_EXTENSION_NAME = 'prettier';

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
 * - .editorconfig
 * - Extension settings
 * - Prettier extension settings
 */
function activate(context) {
	const prettierConfig = vscode.workspace.getConfiguration(PRETTIER_EXTENSION_NAME);
	const config = vscode.workspace.getConfiguration(EXTENSION_NAME);

	worker = new Worker(path.join(__dirname, 'formatter-worker.js'));
	const langs = [
		{ scheme: 'file', language: 'move' },
		{ scheme: 'untitled', language: 'move' },
	];

	context.subscriptions.push(
		vscode.languages.registerDocumentRangeFormattingEditProvider(langs, {
			provideDocumentRangeFormattingEdits: async (document, range, _opts, token) => {
				const options = findMatchingConfig(document.uri);

				// send the text and options to the worker
				worker.postMessage(JSON.stringify({ text: document.getText(), options }));

				// wait for the worker to send the formatted text back. If it
				// takes longer than 5 seconds, reject the promise.
				const edited = await new Promise((resolve, reject) => {
					setTimeout(() => reject(), WAIT_TIME);
					worker.once('message', ({ text, message }) => {
						message && channel.appendLine(message);
						resolve(text);
					});
				});

				return [vscode.TextEdit.replace(range, edited)];
			},
		}),
	);
}

/**
 * For the given filepath, seach for one of the following configuration files:
 * - .prettierrc (prettier.json etc)
 * - .editorconfig
 *
 * Alternatively use (in order, if set):
 * - Extension settings
 * - Prettier extension settings
 */
function findMatchingConfig(documentUri) {
	const root = vscode.workspace.getWorkspaceFolder(documentUri).uri.path;
	let lookup = documentUri.path;
	let search = {};

	// go back in the directory until the root is found; or until we find the
	// .editorconfig or .prettierrc file
	while (lookup !== root) {
		lookup = path.join(lookup, '..');

		const editorConfig = cosmiconfig('editorconfig').search(lookup);
		const prettierConfig = cosmiconfig('prettier').search(lookup);

		if (editorConfig || prettierConfig) {
			channel.appendLine(`Found a configuration file in ${lookup}`);
			editorConfig && channel.appendLine(`EditorConfig: ${editorConfig.filepath}`);
			prettierConfig && channel.appendLine(`PrettierConfig: ${prettierConfig.filepath}`);
			search = prettierConfig.config || editorConfig.config;
			break;
		}
	}

	const prettierConfig = vscode.workspace.getConfiguration(PRETTIER_EXTENSION_NAME);
	const formatterConfig = vscode.workspace.getConfiguration(EXTENSION_NAME);

	return {
		...prettierConfig,
		tabWidth: formatterConfig.get('tabWidth'),
		printWidth: formatterConfig.get('printWidth'),
		wrapComments: formatterConfig.get('wrapComments'),
		useModuleLabel: formatterConfig.get('useModuleLabel'),
		autoGroupImports: formatterConfig.get('autoGroupImports'),
		enableErrorDebug: formatterConfig.get('errorDebugMode'),
		...search, // .editorconfig or .prettierrc overrides the extension settings
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
