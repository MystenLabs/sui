import { memo } from 'react';

import styles from './PageLayout.module.css';

import type { ReactNode } from 'react';

type PageLayoutProps = {
    children: ReactNode[] | ReactNode;
};

function PageLayout({ children }: PageLayoutProps) {
    return <div className={styles.page}>{children}</div>;
}

export default memo(PageLayout);
