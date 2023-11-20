import * as assert from 'assert';
import * as Mocha from 'mocha';
import * as path from 'path';
import * as vscode from 'vscode';
import * as lc from 'vscode-languageclient';
import type { MarkupContent } from 'vscode-languageclient';
import { CompletionItemKind } from 'vscode-languageclient';

const isFunctionInCompletionItems = (fnName: string, items: vscode.CompletionItem[]): boolean => {
    return (
        items.find((item) => item.label === fnName && item.kind === CompletionItemKind.Function) !==
        undefined
    );
};

const isKeywordInCompletionItems = (label: string, items: vscode.CompletionItem[]): boolean => {
    return (
        items.find((item) => item.label === label && item.kind === CompletionItemKind.Keyword) !==
        undefined
    );
};

const PRIMITIVE_TYPES = ['u8', 'u16', 'u32', 'u64', 'u128', 'u256', 'bool', 'vector'];

Mocha.suite('LSP', () => {
    Mocha.test('textDocument/documentSymbol', async () => {
        const ext = vscode.extensions.getExtension('move.move-analyzer');
        assert.ok(ext);

        await ext.activate(); // Synchronous waiting for activation to complete

        // 1. get workdir
        const workDir = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '';

        // 2. open doc
        const docs = await vscode.workspace.openTextDocument(path.join(workDir, 'sources/M1.move'));
        await vscode.window.showTextDocument(docs);

        // 3. execute command
        const params: lc.DocumentSymbolParams = {
            textDocument: {
                uri: docs.uri.toString(),
            },
        };

        const syms: Array<lc.DocumentSymbol> | undefined = await
            vscode.commands.executeCommand(
                'move-analyzer.textDocumentDocumentSymbol', params,
            );

        assert.ok(syms);
        assert.deepStrictEqual(syms[0]?.kind, lc.SymbolKind.Module);
        assert.deepStrictEqual(syms[0].name, 'M1');

        assert.ok(syms[0].children);
        assert.deepStrictEqual(syms[0]?.children[0]?.kind, lc.SymbolKind.Constant);
        assert.deepStrictEqual(syms[0]?.children[0].name, 'SOME_CONST');
        assert.deepStrictEqual(syms[0]?.children[1]?.kind, lc.SymbolKind.Struct);
        assert.deepStrictEqual(syms[0]?.children[1].name, 'SomeOtherStruct');
        assert.ok(syms[0].children[1].children);
        assert.deepStrictEqual(syms[0]?.children[1]?.children[0]?.kind, lc.SymbolKind.Field);
        assert.deepStrictEqual(syms[0]?.children[1]?.children[0]?.name, 'some_field');
        assert.deepStrictEqual(syms[0]?.children[1].name, 'SomeOtherStruct');
        assert.deepStrictEqual(syms[0]?.children[2]?.kind, lc.SymbolKind.Function);
        assert.deepStrictEqual(syms[0]?.children[2].name, 'some_other_struct');
        assert.deepStrictEqual(syms[0]?.children[3]?.kind, lc.SymbolKind.Function);
        assert.deepStrictEqual(syms[0]?.children[3].name, 'this_is_a_test');
        assert.deepStrictEqual(syms[0]?.children[3]?.detail, '["test", "expected_failure"]');
    });

    Mocha.test('textDocument/hover for definition in the same module', async () => {
        const ext = vscode.extensions.getExtension('move.move-analyzer');
        assert.ok(ext);

        await ext.activate(); // Synchronous waiting for activation to complete

        // 1. get workdir
        const workDir = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '';

        // 2. open doc
        const docs = await vscode.workspace.openTextDocument(
            path.join(workDir, 'sources/M2.move'),
        );
        await vscode.window.showTextDocument(docs);

        // 3. execute command
        const params: lc.HoverParams = {
            textDocument: {
                uri: docs.uri.toString(),
            },
            position: {
                line: 12,
                character: 8,
            },
        };

        const hoverResult: lc.Hover | undefined =
            await vscode.commands.executeCommand(
                'move-analyzer.textDocumentHover',
                params,
            );

        assert.ok(hoverResult);
        assert.deepStrictEqual((hoverResult.contents as MarkupContent).value,
            // eslint-disable-next-line max-len
            'fun Symbols::M2::other_doc_struct(): Symbols::M3::OtherDocStruct\n\n\nThis is a multiline docstring\n\nThis docstring has empty lines.\n\nIt uses the ** format instead of ///\n\n');

    });

    Mocha.test('textDocument/hover for definition in an external module', async () => {
        const ext = vscode.extensions.getExtension('move.move-analyzer');
        assert.ok(ext);

        await ext.activate(); // Synchronous waiting for activation to complete

        // 1. get workdir
        const workDir = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '';

        // 2. open doc
        const docs = await vscode.workspace.openTextDocument(
            path.join(workDir, 'sources/M2.move'),
        );
        await vscode.window.showTextDocument(docs);

        // 3. execute command
        const params: lc.HoverParams = {
            textDocument: {
                uri: docs.uri.toString(),
            },
            position: {
                line: 18,
                character: 35,
            },
        };

        const hoverResult: lc.Hover | undefined =
            await vscode.commands.executeCommand(
                'move-analyzer.textDocumentHover',
                params,
            );


        assert.ok(hoverResult);
        assert.deepStrictEqual((hoverResult.contents as MarkupContent).value,
            'Symbols::M3::OtherDocStruct\n\nDocumented struct in another module\n');
    });

    Mocha.test('textDocument/completion', async () => {
        const ext = vscode.extensions.getExtension('move.move-analyzer');
        assert.ok(ext);

        await ext.activate(); // Synchronous waiting for activation to complete

        // 1. get workdir
        const workDir = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '';

        // 2. open doc
        const docs = await vscode.workspace.openTextDocument(
            path.join(workDir, 'sources/Completions.move'),
        );
        await vscode.window.showTextDocument(docs);

        // 3. execute command
        const params: lc.CompletionParams = {
            textDocument: {
                uri: docs.uri.toString(),
            },
            position: {
                line: 12,
                character: 1,
            },
        };

        const items = await vscode.commands.executeCommand<Array<vscode.CompletionItem>>(
            'move-analyzer.textDocumentCompletion',
            params,
        );

        assert.ok(items);

        // Items should return all functions defined in the file
        assert.strictEqual(isFunctionInCompletionItems('add', items), true);
        assert.strictEqual(isFunctionInCompletionItems('subtract', items), true);
        assert.strictEqual(isFunctionInCompletionItems('divide', items), true);

        // Items also include all primitive types because they are keywords
        PRIMITIVE_TYPES.forEach((primitive) => {
            assert.strictEqual(isKeywordInCompletionItems(primitive, items), true);
        });

        const colonParams: lc.CompletionParams = {
            textDocument: {
                uri: docs.uri.toString(),
            },
            // The position of the character ":"
            position: {
                line: 9,
                character: 15,
            },
        };

        const itemsOnColon = await vscode.commands.executeCommand<Array<vscode.CompletionItem>>(
            'move-analyzer.textDocumentCompletion',
            colonParams,
        );

        assert.ok(itemsOnColon);

        const keywordsOnColon = itemsOnColon.filter(i => i.kind === CompletionItemKind.Keyword);
        // Primitive types are the only keywords returned after inserting the colon
        assert.strictEqual(keywordsOnColon.length, PRIMITIVE_TYPES.length);

        // Final safety check
        PRIMITIVE_TYPES.forEach((primitive) => {
            assert.strictEqual(isKeywordInCompletionItems(primitive, keywordsOnColon), true);
        });
    });
});
