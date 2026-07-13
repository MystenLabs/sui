// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { treeFn } from '../../printer';
import { AstPath, doc } from 'prettier';
import { emptyBlockOrList } from '../../utilities';

/** The type of the node implemented in this file */
export const NODE_TYPE = 'unit_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        // `()` has no named children, but may contain comments
        return (path) => emptyBlockOrList(path, '(', ')', doc.builders.line);
    }

    return null;
}
