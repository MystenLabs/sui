module migration::migration {

    public fun t() { abort migration::validate::make_error_code() }

}
