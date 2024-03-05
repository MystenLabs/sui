// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_function)]
module ml::linear_regression {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{TxContext};
    use ml::ifixed_point32::{IFixedPoint32, zero, from_rational, add, subtract, multiply, divide, divide_by_constant};

    struct Model has key, store {
        id: UID,
        mean_x: IFixedPoint32,
        mean_y: IFixedPoint32,
        var_x: IFixedPoint32,
        cov_xy: IFixedPoint32,
        n: u64,
    }

    /// Create a shared-object Game.
    public entry fun create(ctx: &mut TxContext) {
        let model = Model {
            id: object::new(ctx),
            mean_x: zero(),
            mean_y: zero(),
            var_x: zero(),
            cov_xy: zero(),
            n: 0,
        };
        transfer::public_share_object(model);
    }

    /// Anyone can close the game by providing the randomness of round-2.
    public entry fun submit_point(model: &mut Model, x: u64, y: u64) {

        let x_fixed = from_rational(x, 1, false);
        let y_fixed = from_rational(y, 1, false);

        model.n = model.n + 1;

        let dx = subtract(x_fixed, model.mean_x);
        let dy = subtract(y_fixed, model.mean_y);
        model.mean_x = divide_by_constant(add(model.mean_x, x_fixed), model.n);
        model.mean_y = divide_by_constant(add(model.mean_y, y_fixed), model.n);
        model.var_x = divide_by_constant(add(model.var_x, subtract(multiply(multiply(from_rational(model.n-1, model.n, false), dx), dx), model.var_x)), model.n);
        model.cov_xy = divide_by_constant(add(model.cov_xy, subtract(multiply(multiply(from_rational(model.n-1, model.n, false), dx), dy), model.cov_xy)), model.n);
    }

    public entry fun get_alpha(model: &mut Model): IFixedPoint32 {
        divide(model.cov_xy, model.var_x)   
    }

    public entry fun get_beta(model: &mut Model): IFixedPoint32 {
        subtract(model.mean_y, multiply(get_alpha(model), model.mean_x))
    }
}
