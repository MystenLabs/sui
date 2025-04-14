module specs::random_spec;

use sui::random::{new_generator, u128_in_range, Random, RandomGenerator};
use sui::tx_context::TxContext;
use prover::prover::{asserts, ensures};

#[spec(target = sui::random::new_generator)]
fun new_generator_spec(r: &Random, ctx: &mut TxContext): RandomGenerator {
    new_generator(r, ctx)
}

#[spec(target = sui::random::u128_in_range)]
fun u128_in_range_spec(g: &mut RandomGenerator, min: u128, max: u128, num_of_bytes: u8): u128 {
    asserts(min <= max);
    let result = u128_in_range(g, min, max, num_of_bytes);
    ensures(result >= min);
    ensures(result <= max);
    result
}
