// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Exercises Move 2024 language features (enums, `match`, macros, positional
/// structs) so the corresponding bytecode (e.g. `variant_nodes`) is executed.
module move_building_blocks::language_features {
    public enum Shape has store, drop, copy {
        Circle(u64),
        Rectangle(u64, u64),
        Empty,
    }

    /// Positional struct.
    public struct Pair(u64, u64) has store, drop, copy;

    public struct ShapeHolder has key, store {
        id: UID,
        shape: Shape,
        bounds: Pair,
        area: u64,
    }

    macro fun square($x: u64): u64 {
        $x * $x
    }

    public fun create(kind: u8, a: u64, b: u64, ctx: &mut TxContext) {
        // Bound inputs so area computations cannot overflow u64.
        let a = a % 100_000;
        let b = b % 100_000;
        let shape = make_shape(kind, a, b);
        let computed_area = area(&shape);
        let holder = ShapeHolder {
            id: object::new(ctx),
            shape,
            bounds: Pair(a, b),
            area: computed_area,
        };
        transfer::share_object(holder);
    }

    public fun update(holder: &mut ShapeHolder, kind: u8, a: u64, b: u64) {
        let a = a % 100_000;
        let b = b % 100_000;
        let shape = make_shape(kind, a, b);
        holder.area = area(&shape);
        holder.shape = shape;
        holder.bounds = Pair(a, b);
    }

    fun make_shape(kind: u8, a: u64, b: u64): Shape {
        match (kind % 3) {
            0 => Shape::Circle(a),
            1 => Shape::Rectangle(a, b),
            _ => Shape::Empty,
        }
    }

    fun area(shape: &Shape): u64 {
        match (shape) {
            Shape::Circle(r) => square!(*r),
            Shape::Rectangle(w, h) => *w * *h,
            Shape::Empty => 0,
        }
    }
}
