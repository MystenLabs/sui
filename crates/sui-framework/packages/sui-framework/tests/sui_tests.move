#[test_only]
module sui::sui_tests {
  use std::ascii;
  use std::string;
  use std::option;
  use sui::sui;
  use sui::url;
  use sui::coin;
  use sui::tx_context;
  use sui::test_utils::destroy;
  use sui::test_utils::assert_eq;

  const SENDER: address = @0x0;
  const SUI_DECIMALS: u8 = 9;
  const SUI_TOTAL_SUPPLY: u64 = 10_000_000_000_000_000_000;

  #[test]
  fun init_sui_currency() {
    let ctx = tx_context::dummy();
    let metadata = sui::new_metadata_for_testing(&mut ctx);

    let decimals = coin::get_decimals(&metadata);
    let symbol_bytes = ascii::as_bytes(&coin::get_symbol(&metadata));
    let name_bytes = string::bytes(&coin::get_name(&metadata));
    let description_bytes = string::bytes(&coin::get_description(&metadata));
    let icon_url = ascii::as_bytes(&url::inner_url(option::borrow(&coin::get_icon_url(&metadata))));

    assert_eq(decimals, SUI_DECIMALS);
    assert_eq(*symbol_bytes, b"SUI");
    assert_eq(*name_bytes, b"Sui");
    assert_eq(*description_bytes, b"");
    assert_eq(*icon_url, vector[]);

    destroy(metadata);
  }
}
