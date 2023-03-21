// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type LinkData = {
    href: string;
    display: string;
};

function toLinkData(link: string): LinkData | string | null {
    try {
        const url = new URL(link);
        return { href: link, display: url.hostname };
    } catch (e) {
        return link || null;
    }
}

export function processDisplay(display: Record<string, string> | null) {
    if (display) {
        const { name, description, creator, img_url, link, project_url } =
            display;
        return {
            name: name || null,
            description: description || null,
            imageUrl: img_url || null,
            link: link ? toLinkData(link) : null,
            projectUrl: project_url ? toLinkData(project_url) : null,
            creator: creator ? toLinkData(creator) : null,
        };
    }
    return null;
}
