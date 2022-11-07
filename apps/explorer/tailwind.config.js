// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    ...require('@mysten/core/tailwind.config'),

    /*
     * NOTE: The Tailwind CSS reset doesn't mix well with the existing styles.
     * We currently disable the CSS reset and expect components to adapt accordingly.
     * When we fix this, we should use the following as a CSS reset: @tailwind base;
     */
    corePlugins: {
        preflight: false,
    },
};
