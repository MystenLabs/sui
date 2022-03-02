import cn from 'classnames';
import { memo } from 'react';

import styles from './TruncatedLabel.module.css';

type TruncatedLabelProps = {
    label: string;
    size?: 'small' | 'normal';
};

function TruncatedLabel({ label, size = 'normal' }: TruncatedLabelProps) {
    return (
        <span className={cn(styles.label, styles[size])} title={label}>
            {label}
        </span>
    );
}

export default memo(TruncatedLabel);
