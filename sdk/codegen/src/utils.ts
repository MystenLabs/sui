// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import ts, { SyntaxKind } from 'typescript';

export function printStatements(statements: ts.Statement[]) {
	const nodes = ts.factory.createNodeArray(
		statements.filter((statement) => statement.kind !== SyntaxKind.EmptyStatement),
	);
	const printer = ts.createPrinter({});
	const sourcefile = ts.createSourceFile(
		'file.ts',
		'',
		ts.ScriptTarget.ESNext,
		false,
		ts.ScriptKind.TS,
	);

	return printer.printList(ts.ListFormat.SourceFileStatements, nodes, sourcefile);
}

export function printExpression(expression: ts.Expression) {
	const statement = ts.factory.createExpressionStatement(expression);

	return printStatements([statement]);
}

type TSTemplateValue = string | number | boolean | ts.Statement[] | ts.Expression;

export function parseTS(strings: TemplateStringsArray, ...values: TSTemplateValue[]) {
	const source = strings.reduce((acc, str, i) => {
		if (typeof values[i] === 'object') {
			if (Array.isArray(values[i])) {
				return `${acc}${str}${printStatements(values[i])}`;
			}

			if (ts.isExpression(values[i])) {
				return `${acc}${str}${printExpression(values[i])}`;
			}
		}

		return `${acc}${str}${values[i] ?? ''}`;
	}, '');

	const lines = source.replace(/^\s/m, '').split('\n');
	const firstLine = lines[0];
	const indent = firstLine.match(/^\s*/)?.[0] ?? '';
	const unIndented = lines.map((line) => line.replace(indent, '')).join('\n');

	const sourceFile = ts.createSourceFile('file.ts', unIndented, ts.ScriptTarget.Latest, false);

	return [...sourceFile.statements.values()];
}

export function mapToObject<T>(
	items: Iterable<T>,
	mapper: (item: T) => null | [string, TSTemplateValue],
) {
	const fieldProps = [...items]
		.map(mapper)
		.filter((value) => value !== null)
		.map(([key, value]) => {
			const node = parseTS/* ts */ `({${key}: ${value}})`;
			if (!node) {
				throw new Error('Expected node');
			}

			if (!ts.isExpressionStatement(node[0])) {
				throw new Error('Expected Expression statement');
			}

			if (!ts.isParenthesizedExpression(node[0].expression)) {
				throw new Error('Expected Parenthesized Expression');
			}

			if (!ts.isObjectLiteralExpression(node[0].expression.expression)) {
				throw new Error('Expected Object Literal Expression');
			}

			return node[0].expression.expression.properties[0];
		});

	return ts.factory.createObjectLiteralExpression(fieldProps, true);
}
