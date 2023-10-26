#[test_only]
module sui::sui_tests {
  use sui::sui;
  use sui::coin;
  use sui::balance;
  use sui::test_scenario;
  use sui::test_utils::assert_eq;

  #[test]
  fun init_sui_currency() {
    let scenario = test_scenario::begin(@0x0);
    let test = &mut scenario;

    test_scenario::next_tx(test, @0x0); 
    {
      let sui_balance = sui::new_for_testing(test_scenario::ctx(test));
      assert_eq(balance::destroy_for_testing(sui_balance), 10_000_000_000_000_000_000);
    };

    test_scenario::next_tx(test, @0x0);
    {
      let sui_coin_metadata = test_scenario::take_immutable<coin::CoinMetadata<sui::SUI>>(test);

      assert_eq(coin::get_decimals(&sui_coin_metadata), 9);

      test_scenario::return_immutable(sui_coin_metadata);
    };
    test_scenario::end(scenario);
  }
}