// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const COPYRIGHT = `
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
`;

/** @type {import('@svgr/core').Config} */
module.exports = {
  icon: true,
  typescript: true,
  outDir: "./src",
  jsxRuntime: 'automatic',
  replaceAttrValues: {
    "#383F47": "currentColor",
  },
  template(variables, { tpl }) {
    return tpl`
    ${COPYRIGHT}
    ${variables.imports};

    ${variables.interfaces};

    const ${variables.componentName} = (${variables.props}) => (
      ${variables.jsx}
    );

    ${variables.exports};
    `;
  },
};
