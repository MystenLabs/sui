// tests an incorrect attribute for warning supression

#[allow(all(), unused = true, unused_(assignment, variable))]
module 0x42::m {
}
