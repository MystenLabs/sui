// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Highlight, { defaultProps } from 'prism-react-renderer';
import 'prism-themes/themes/prism-one-light.css';

import type { Language } from 'prism-react-renderer';

interface Props {
    code: string;
    language: Language;
}

export function SyntaxHighlighter({ code, language }: Props) {
    return (
        <section className="px-0 ">
            <div className="overflow-auto whitespace-pre p-2 pl-0 pr-0 font-mono text-sm">
                <Highlight
                    {...defaultProps}
                    code={code}
                    language={language}
                    theme={undefined}
                >
                    {({ style, tokens, getLineProps, getTokenProps }) => (
                        <pre
                            className="bg-transparent !p-0 font-medium"
                            style={style}
                        >
                            {tokens.map((line, i) => (
                                <div
                                    {...getLineProps({ line, key: i })}
                                    key={i}
                                    className="table-row"
                                >
                                    <div className="table-cell select-none pr-4 text-left opacity-50">
                                        {i + 1}
                                    </div>

                                    {line.map((token, key) => (
                                        <span
                                            {...getTokenProps({
                                                token,
                                                key,
                                            })}
                                            key={key}
                                        />
                                    ))}
                                </div>
                            ))}
                        </pre>
                    )}
                </Highlight>
            </div>
        </section>
    );
}
