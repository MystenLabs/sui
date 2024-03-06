// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_function)]
module ml::logistic_regression {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{TxContext};
    use ml::ifixed_point32::{IFixedPoint32, zero, one, add, multiply, divide, exp, subtract};
    use std::vector;

    struct Model has key, store {
        id: UID,
        b: vector<IFixedPoint32>,
        k: u64,
    }

    /// Create a shared-object Game.
    public entry fun create(k: u64, ctx: &mut TxContext) {
        let b: vector<IFixedPoint32> = vector::empty();
        let i = 0;
        while (i < k) {
            vector::push_back(&mut b, zero());
            i = i + 1;
        };
        let model = Model {
            id: object::new(ctx),
            b: b,
            k: k,
        };
        transfer::public_share_object(model);
    }

    public fun evaluate(b: &vector<IFixedPoint32>, x: &vector<IFixedPoint32>): IFixedPoint32 {
        assert!(vector::length(x) == vector::length(b) - 1, 0);
        let sum = *vector::borrow(b, 0);
        let i = 1;
        while (i < vector::length(b)) {
            sum = add(sum, multiply(*vector::borrow(b, i), *vector::borrow(x, i - 1)));
            i = i + 1;
        };
        let e = exp(sum);
        divide(e, add(one(), e))
    }
    
    public fun predict(model: &mut Model, x: &vector<IFixedPoint32>): IFixedPoint32 {
        evaluate(&model.b, x)
    }

    fun row_gradient(model: &mut Model, rate: IFixedPoint32, x: &vector<IFixedPoint32>, expected: IFixedPoint32): vector<IFixedPoint32> {
        assert!(vector::length(x) == model.k - 1, 0);
        let y_hat = predict(model, x);
        let error = subtract(expected, y_hat);
        let t = multiply(multiply(y_hat, subtract(one(), y_hat)), multiply(rate, error));

        let i = 0;
        let delta: vector<IFixedPoint32> = vector::empty();

        while (i < model.k) {
            let delta_i = t;
            if (i > 0) {
                delta_i = multiply(delta_i, *vector::borrow(x, i - 1));
            };
            vector::push_back(&mut delta, delta_i);
            i = i + 1;
        };
        delta
    }

    /// Fit model using SGD on the given data points
    public fun fit(model: &mut Model, x: &vector<vector<IFixedPoint32>>, expected: &vector<IFixedPoint32>, rate: IFixedPoint32, rounds: u64) {
        assert!(vector::length(x) == vector::length(expected), 0);

        let i = 0;
        while (i < rounds) {
            let j = 0;
            while (j < vector::length(x)) {
                fit_row(model, rate, vector::borrow(x, j), *vector::borrow(expected, j));
                j = j + 1;
            };
            i = i + 1;
        }
    }

    /// Fit a single row
    fun fit_row(model: &mut Model, rate: IFixedPoint32, x: &vector<IFixedPoint32>, expected: IFixedPoint32) {
        let delta = row_gradient(model, rate, x, expected);

        let new_b: vector<IFixedPoint32> = vector::empty();
        let i = 0;
        while (i < model.k) {
            let new_bi = add(*vector::borrow(&model.b, i), *vector::borrow(&delta, i));
            vector::push_back(&mut new_b, new_bi);
            i = i + 1;
        };    
        model.b = new_b;
    }

    public fun get_coefficients(model: &Model): vector<IFixedPoint32> {
        model.b
    }
}
