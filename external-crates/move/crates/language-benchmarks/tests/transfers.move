module 0x1::bench {

    const COUNT: u64 = 10_000u64;

    public struct Account { balance: u64 }

    fun destroy_account(account: Account) {
        let Account { balance: _ } = account;
    }

    public fun bench() {
        let mut accounts = vector::empty<Account>();
        let num_accounts = COUNT;
        let transfer_amount = 10;

        // Initialize accounts with a balance of 1000 each
        let mut i = 0;
        while (i < num_accounts) {
            accounts.push_back(Account { balance: 1000 });
            i = i + 1;
        };

        // Perform transfers
        let mut j = 0;
        while (j < num_accounts / 2) {
            let sender_index = j;
            let receiver_index = num_accounts - j - 1;

            let sender = accounts.borrow_mut(sender_index);
            if (sender.balance >= transfer_amount) {
                sender.balance = sender.balance - transfer_amount;
                let receiver = accounts.borrow_mut(receiver_index);
                receiver.balance = receiver.balance + transfer_amount;
            };

            j = j + 1;
        };

        let mut i = 0;
        while (i < num_accounts) {
            destroy_account(accounts.pop_back());
            i = i + 1;
        };
        accounts.destroy_empty();
    }
}
