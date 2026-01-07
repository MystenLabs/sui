/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

import React, { PropsWithChildren, useId, useMemo } from "react";
import { useGlossary } from "./GlossaryProvider";
import styles from "./term.module.css";

type TermProps = PropsWithChildren<{
    /** If omitted, we use the child text as the lookup key */
    lookup?: string;
    /** Optional custom definition override for this occurrence */
    definition?: string;
    /** Optional aria-label for accessibility */
    ariaLabel?: string;
}>;

export default function Term({ children, lookup, definition, ariaLabel }: TermProps) {
    const id = useId();
    const { map } = useGlossary();

    const childText = useMemo(() => {
        if (typeof children === "string") return children.trim();
        // Fallback: try to extract text content if children are nested
        if (Array.isArray(children)) {
            return children
                .map((c: any) => (typeof c === "string" ? c : ""))
                .join("")
                .trim();
        }
        return "";
    }, [children]);

    const key = (lookup || childText).toLowerCase();
    const entry = definition ? { label: childText || lookup || key, definition } : map?.get(key);

    if (!entry) {
        // If no match, just render the child as-is so content remains readable
        return <>{children}</>;
    }

    const tooltipId = `term-tip-${id}`;
    return (
        <span className={styles.term} aria-describedby={tooltipId} role="definition">
            {children}
            <span
                className={styles.tooltip}
                id={tooltipId}
                role="tooltip"
                aria-label={ariaLabel || entry.label}
            >
                <strong className={styles.tooltipTitle}>{entry.label}</strong>
                <span className={styles.tooltipBody}>{entry.definition}</span>
            </span>
        </span>
    );
}
