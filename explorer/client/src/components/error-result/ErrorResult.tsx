import theme from '../../styles/theme.module.css';

import styles from './ErrorResult.module.css';

const ErrorResult = ({
    id,
    errorMsg,
}: {
    id: string | undefined;
    errorMsg: string;
}) => {
    return (
        <div className={theme.textresults}>
            <div className={styles.problemrow}>
                <div>{errorMsg}</div>
                <div>{id}</div>
            </div>
        </div>
    );
};

export default ErrorResult;
