/// # Nested Bullet Points
///
/// - Item one
///   - Nested item one-a
///   - Nested item one-b
///     - Deeply nested item one-b-i
///     - Deeply nested item one-b-ii
///   - Nested item one-c
/// - Item two
/// - Item three
///   - Nested item three-a
///
/// # Nested Enumerated Lists
///
/// 1. First item
///    1. Sub-item one
///    2. Sub-item two
///       1. Sub-sub-item one
///       2. Sub-sub-item two
///    3. Sub-item three
/// 2. Second item
/// 3. Third item
///    1. Sub-item one
///
/// # Mixed Nested Lists
///
/// 1. Ordered item one
///    - Unordered sub-item a
///    - Unordered sub-item b
///      1. Back to ordered
///      2. Still ordered
/// 2. Ordered item two
///   - Mixed sub-item
module a::m {
    /// - Top-level bullet
    ///   - Nested bullet in function doc
    ///     - Deeply nested bullet
    /// - Another top-level bullet
    ///
    /// 1. First step
    ///    1. Sub-step one
    ///    2. Sub-step two
    /// 2. Second step
    entry fun nested_list_fn() { }
}
