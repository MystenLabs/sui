#[test_only]
module sui::sui_tests {
  use sui::sui;
  use sui::coin;
  use sui::balance;
  use sui::test_scenario;
  use sui::test_utils::assert_eq;

  const SENDER: address = @0x0;
  const SUI_DECIMALS: u8 = 9;
  const SUI_TOTAL_SUPPLY: u64 = 10_000_000_000_000_000_000;

  #[test]
  fun init_sui_currency() {
    let scenario = test_scenario::begin(SENDER);
    let scenario_mut = &mut scenario;

    test_scenario::next_tx(scenario_mut, SENDER); 
    {
      let sui_balance = sui::new_for_testing(test_scenario::ctx(scenario_mut));
      assert_eq(balance::destroy_for_testing(sui_balance), SUI_TOTAL_SUPPLY);
    };

    test_scenario::next_tx(scenario_mut, SENDER);
    {
      let sui_coin_metadata = test_scenario::take_immutable<coin::CoinMetadata<sui::SUI>>(scenario_mut);

      assert_eq(coin::get_decimals(&sui_coin_metadata), SUI_DECIMALS);

      test_scenario::return_immutable(sui_coin_metadata);
    };

    test_scenario::end(scenario);
  }
}