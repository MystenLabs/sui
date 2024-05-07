module migration::other {
    use migration::migration;

    public fun t() { ::migration::migration::t() }
}
