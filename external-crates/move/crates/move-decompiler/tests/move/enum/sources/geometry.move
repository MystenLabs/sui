module enums::geometry;

public enum Shape has copy, drop {
    Square {
        side: u64,
    },
    Triangle {
        base: u64,
        height: u64
    }
}

const WRONG_SHAPE: u64 = 1;

public fun get_area(shape: Shape): u64 {
    match (shape) {
        Shape::Square { side } => side * side,
        Shape::Triangle { base, height } => base * height / 2,
    }
}

public fun set_triangle_dimensions(shape: &mut Shape, new_base: u64, new_height: u64) {
    match (shape) {
        Shape::Triangle {  base,  height } => {
            *base = new_base;
            *height = new_height;
        },
        _ =>  abort(WRONG_SHAPE),
    }
}

public fun set_square_side(shape: &mut Shape, new_side: u64) {
    match (shape) {
        Shape::Square { side } => {
            *side = new_side;
        },
        _ =>  abort(WRONG_SHAPE),
    }
}

public fun get_square_side(shape: &Shape): u64 {
    match (shape) {
        Shape::Square { side } => *side,
        _ => abort(WRONG_SHAPE),
    }
}

public fun get_triangle_size(shape: &Shape): (u64, u64) {
    match (shape) {
        Shape::Triangle { base, height } => (*base, *height),
        _ => abort(WRONG_SHAPE),
    }
}