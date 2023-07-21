import { AccountSource, AccountSourceSerialized, AccountSourceSerializedUI } from './AccountSource';

type DataDecryptedV0 = {
	version: 0;
	refreshToken: string;
};

interface QredoAccountSourceSerialized extends AccountSourceSerialized {
	type: 'qredo';
	encrypted: string;
}

interface QredoAccountSourceSerializedUI extends AccountSourceSerializedUI {
	type: 'qredo';
}

export class QredoAccountSource extends AccountSource<
	QredoAccountSourceSerialized,
	DataDecryptedV0
> {
	constructor(id: string) {
		super({ type: 'qredo', id });
	}

	toUISerialized(): Promise<QredoAccountSourceSerializedUI> {
		throw new Error('Method not implemented.');
	}
	isLocked(): Promise<boolean> {
		throw new Error('Method not implemented.');
	}
	lock(): Promise<void> {
		throw new Error('Method not implemented.');
	}
}
