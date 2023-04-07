// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import * as vscode from 'vscode';

/**
 * A logger for the VS Code extension.
 *
 * Messages that are logged appear in an output channel created below that is dedicated to the
 * extension (or "client"), in the extension user's "Output View." This logger should be used for
 * messages related to VS Code and this extension, as opposed to messages regarding the language
 * server, which appear in a separate output channel.
 **/
export const log = new class {
    private readonly output = vscode.window.createOutputChannel('Move Analyzer Client');

    /** Log an informational message (as opposed to an error or a warning). */
    info(message: string): void {
        this.write('INFO', message);
    }

    private write(label: string, message: string): void {
        this.output.appendLine(`${label} [${new Date().toLocaleString()}]: ${message}`);
    }
}();
