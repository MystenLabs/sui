// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import cl from 'classnames';
import Highlight, { defaultProps, Prism } from 'prism-react-renderer';
import 'prism-themes/themes/prism-one-light.css';

import codestyle from '../../styles/bytecode.module.css';

import type { Language } from 'prism-react-renderer';

import styles from './TxModuleView.module.css';
// inclue Rust language.
// @ts-ignore
(typeof global !== 'undefined' ? global : window).Prism = Prism;
require('prismjs/components/prism-rust');

function TxModuleView({ itm }: { itm: any }) {
    return (
        <section className={styles.modulewrapper}>
            <div className={styles.moduletitle}>{itm[0]}</div>
            <div className={cl(codestyle.code, styles.codeview)}>
                <Highlight
                    {...defaultProps}
                    code={itm[1]}
                    language={'rust' as Language}
                    theme={undefined}
                >
                    {({
                        className,
                        style,
                        tokens,
                        getLineProps,
                        getTokenProps,
                    }) => (
                        <pre className={className} style={style}>
                            {tokens.map((line, i) => (
                                <div
                                    {...getLineProps({ line, key: i })}
                                    key={i}
                                    className={styles.codeline}
                                >
                                    <div className={styles.codelinenumbers}>
                                        {i + 1}
                                    </div>
                                    {line.map((token, key) => (
                                        <span
                                            {...getTokenProps({ token, key })}
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

export default TxModuleView;
