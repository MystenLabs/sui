/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/


import React, { useMemo } from "react";
import { useGlossary } from "./GlossaryProvider";

export default function GlossaryPage() {
    const { map } = useGlossary();

    const { sortedTerms, termsByLetter, availableLetters } = useMemo(() => {
        if (!map)
            return {
                sortedTerms: [],
                termsByLetter: {},
                availableLetters: new Set(),
            };

        // Extract unique entries (avoid duplicates from aliases/ids)
        const uniqueEntries = new Map();
        map.forEach((entry) => {
            if (!uniqueEntries.has(entry.label)) {
                uniqueEntries.set(entry.label, entry);
            }
        });

        // Convert to array and sort alphabetically by label
        const sorted = Array.from(uniqueEntries.values()).sort((a, b) =>
            a.label.localeCompare(b.label, undefined, { sensitivity: "base" }),
        );

        // Group by first letter
        const byLetter: Record<string, typeof sorted> = {};
        const letters = new Set<string>();

        sorted.forEach((term) => {
            const firstLetter = term.label[0].toUpperCase();
            if (!byLetter[firstLetter]) {
                byLetter[firstLetter] = [];
            }
            byLetter[firstLetter].push(term);
            letters.add(firstLetter);
        });

        return {
            sortedTerms: sorted,
            termsByLetter: byLetter,
            availableLetters: letters,
        };
    }, [map]);

    const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("");

    if (!map) {
        return (
            <div className="text-center py-8 text-wal-gray-80 dark:text-wal-white-60 italic">
                Loading glossary...
            </div>
        );
    }

    if (sortedTerms.length === 0) {
        return (
            <div className="text-center py-8 text-wal-gray-80 dark:text-wal-white-60 italic">
                No glossary terms found.
            </div>
        );
    }

    return (
        <div className="p-4">
            {/* Alphabet Navigation */}
            <div className="mb-8 p-4 bg-wal-gray-10 dark:bg-wal-white-20 rounded-lg">
                <div className="flex flex-wrap gap-2 justify-center">
                    {alphabet.map((letter) => {
                        const hasTerms = availableLetters.has(letter);
                        return (
                            <a
                                key={letter}
                                href={hasTerms ? `#letter-${letter}` : undefined}
                                className={`
                px-2 py-1 text-sm font-medium rounded transition-colors
                ${
                    hasTerms
                        ? "hover:bg-blue-50 dark:text-wal-link dark:hover:text-wal-link-hover dark:hover:bg-wal-gray-30 cursor-pointer"
                        : "text-wal-gray-20 dark:text-wal-white-10 cursor-default"
                }
                `}
                                {...(!hasTerms && { onClick: (e) => e.preventDefault() })}
                            >
                                {letter}
                            </a>
                        );
                    })}
                </div>
            </div>

            {/* Terms grouped by letter */}
            <div className="flex flex-col gap-8">
                {alphabet.map((letter) => {
                    const letterTerms = termsByLetter[letter];
                    if (!letterTerms) return null;

                    return (
                        <div key={letter} id={`letter-${letter}`}>
                            <h2
                                className={
                                    "text-2xl font-bold text-wal-gray-80 " +
                                    "dark:text-wal-white-60 mb-4 pb-2 border-b-2 " +
                                    "border-wal-green-dark dark:border-wal-green-light"
                                }
                            >
                                {letter}
                            </h2>
                            <div className="flex flex-col gap-4">
                                {letterTerms.map((term) => (
                                    <div
                                        key={term.label}
                                        className="border-b border-wal-gray-50 dark:border-wal-white-30 pb-4 last:border-b-0 last:pb-0"
                                    >
                                        <dt className="text-xl font-semibold text-wal-gray-80 dark:text-wal-white-60 mb-2">
                                            {term.label}
                                        </dt>
                                        <dd className="m-0 leading-relaxed text-wal-gray-80 dark:text-wal-white-60">
                                            {term.definition}
                                        </dd>
                                    </div>
                                ))}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
