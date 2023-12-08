module 0x42::M {
    // not allowed
    use 0x42::M as Self;
    // now allowed
    use 0x42::M as vector;
}
