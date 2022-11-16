#[test_only]
module sui::rand_tests {
    // use std::debug;
    use sui::test_scenario;
    use sui::tx_context::TxContext;
    use sui::rand;

    const EOUTSIDE_RANGE: u64 = 0;
    const EONE_IN_A_MILLION_ERROR: u64 = 1;

    public fun print_rand(min: u64, max: u64, ctx: &mut TxContext): u64 {
        let num = rand::rng(min, max, ctx);
        // debug::print(&num);
        assert!(num >= min && num < max, EOUTSIDE_RANGE);
        num
    }

    #[test]
    public fun test1() {
        // 1st tx: must always be == 1
        let scenario = test_scenario::begin(@0x5);
        print_rand(1, 2, test_scenario::ctx(&mut scenario));

        // 2nd tx
        test_scenario::next_tx(&mut scenario, @0x5);
        print_rand(15, 99, test_scenario::ctx(&mut scenario));

        // 3rd tx
        test_scenario::next_tx(&mut scenario, @0x5);
        let r1 = print_rand(99, 1000000, test_scenario::ctx(&mut scenario));

        // 4th tx: identical range as above tx, but different outcome
        test_scenario::next_tx(&mut scenario, @0x5);
        let r2 = print_rand(99, 1000000, test_scenario::ctx(&mut scenario));
        assert!(r1 != r2, EONE_IN_A_MILLION_ERROR);

        // 5th tx: 100 rands in the same tx
        test_scenario::next_tx(&mut scenario, @0x5);
        let i = 0;
        while (i < 100) {
            print_rand(0, 100, test_scenario::ctx(&mut scenario));
            i = i + 1;
        };

        test_scenario::end(scenario);
    }
}