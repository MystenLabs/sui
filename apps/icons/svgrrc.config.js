// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const path = require('path');

const COPYRIGHT = `
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
`;

/** @type {import('@svgr/core').Config} */
module.exports = {
	// The default parser set by svgr is `babel`, which makes the import sorting plugin fail.
	prettierConfig: {
		parser: 'babel-ts',
	},
	icon: true,
	typescript: true,
	outDir: './src',
	jsxRuntime: 'automatic',
	replaceAttrValues: {
		'#383F47': 'currentColor',
		'#007195': 'currentColor',
	},
	indexTemplate(filePaths) {
		const exportEntries = filePaths.map((filePath) => {
			const basename = path.basename(filePath, path.extname(filePath));
			const exportName = /^\d/.test(basename) ? `Svg${basename}` : basename;
			return `export { default as ${exportName} } from './${basename}'`;
		});
		return COPYRIGHT + exportEntries.join('\n');
	},
	template(variables, { tpl }) {
		const template = tpl`
    ${variables.imports};

    ${variables.interfaces};

    const ${variables.componentName} = (${variables.props}) => (
      ${variables.jsx}
    );

    ${variables.exports};
    `;

		// Insert the copyright header, attached to the first node:
		template[0].leadingComments = [
			{ type: 'CommentLine', value: ' Copyright (c) Mysten Labs, Inc.' },
			{ type: 'CommentLine', value: ' SPDX-License-Identifier: Apache-2.0' },
		];

		return template;
	},
};
