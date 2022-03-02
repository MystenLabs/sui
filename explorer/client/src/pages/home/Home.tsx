import PageLayout from '../../components/page-layout/PageLayout';
import TransactionsTable from '../../components/transactions/transactions-table/TransactionsTable';
import { TransactionStatus } from '../../components/transactions/types';
import txs from '../../utils/transaction_mock.json';

import type { TransactionType } from '../../components/transactions/types';

// TODO: mock data should be removed
const latestTxs: TransactionType[] = Array.from(
    { length: 15 },
    (_, index) =>
        (txs.data as TransactionType[])[index] || {
            id: `tx${index}-sdasjdsajjdfladsjdajdaslnjkdaskdasljadslkjasdlk`,
            sender: `sender${index}ldaskdjaopdasldaslkasljkaadslkadslkaslkd`,
            status:
                Math.random() < 0.5
                    ? TransactionStatus.success
                    : TransactionStatus.fail,
            created: [
                `cr-obj${index}-1-asdasdhjsladkjfhaskdjfhjkasdfhasf`,
                `cr-obj${index}-2-asdasdhjsladkjfhaskdjfhjkasdfhasf`,
                `cr-obj${index}-3-asdasdhjsladkjfhaskdjfhjkasdfhasf`,
            ],
            mutated: [
                `mut-obj${index}-1-asdasdhjsladkjfhaskdjfhjkasdfhasf`,
                `mut-obj${index}-2-asdasdhjsladkjfhaskdjfhjkasdfhasf`,
            ],
            deleted: [
                `del-obj${index}-1-asdasdhjsladkjfhaskdjfhjkasdfhasf`,
                `del-obj${index}-2-asdasdhjsladkjfhaskdjfhjkasdfhasf`,
            ],
        }
);

function Home() {
    return (
        <PageLayout>
            <h2>Latest Transactions</h2>
            <TransactionsTable transactions={latestTxs} />
        </PageLayout>
    );
}

export default Home;
