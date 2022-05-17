import { useCallback, useContext, useState } from 'react';

import { NetworkContext } from '../../context';
import { Network } from '../../utils/api/DefaultRpcClient';

import styles from './Network.module.css';

export default function NetworkSelect() {
    const [network, setNetwork] = useContext(NetworkContext);
    const [isModuleOpen, setModuleOpen] = useState(false);

    const openModal = useCallback(
        () => (isModuleOpen ? setModuleOpen(false) : setModuleOpen(true)),
        [isModuleOpen, setModuleOpen]
    );
    const closeModal = useCallback(() => setModuleOpen(false), [setModuleOpen]);

    const chooseNetwork = useCallback(
        (specified: Network) => () =>
            network !== specified ? setNetwork(specified) : null,
        [network, setNetwork]
    );

    const networkStyle = (iconNetwork: Network) =>
        network === iconNetwork ? styles.active : styles.inactive;

    return (
        <div>
            <div onClick={openModal} className={styles.networkbox}>
                {network}
            </div>
            <div
                className={isModuleOpen ? styles.opennetworkbox : styles.remove}
            >
                <div className={styles.opennetworkdetails}>
                    <div className={styles.closeicon} onClick={closeModal}>
                        &times;
                    </div>
                    <h2>Choose a Network</h2>
                    <div
                        onClick={chooseNetwork(Network.Devnet)}
                        className={networkStyle(Network.Devnet)}
                    >
                        Devnet
                    </div>
                    <div
                        onClick={chooseNetwork(Network.Local)}
                        className={networkStyle(Network.Local)}
                    >
                        Local
                    </div>
                </div>
                <div className={styles.detailsbg} onClick={closeModal}></div>
            </div>
        </div>
    );
}
