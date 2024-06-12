
module sui::config {

    use sui::dynamic_field as field;

    // #[error]
    // const EAlreadySetForEpoch: vector<u8> =
    //     b"Setting was already updated at this epoch for the provided Config";
    const EAlreadySetForEpoch: u64 = 0;

    // #[error]
    // const ENotSetForEpoch: vector<u8> =
    //     b"Setting was not updated at this epoch for the provided Config";
    const ENotSetForEpoch: u64 = 1;

    // #[error]
    // const ENotSetForEpoch: vector<u8> = b"Could not read setting for the provided Config";
    #[allow(unused_const)]
    const EReadSettingFailed: u64 = 2;

    public struct Config<phantom WriteCap> has key {
        id: UID,
    }

    public struct Setting<Value: copy + drop + store> has key, store {
        id: UID,
        data: Option<SettingData<Value>>,
    }

    public struct SettingData<Value: copy + drop + store> has store {
        newer_value_epoch: u64,
        newer_value: Value,
        older_value_opt: Option<Value>,
    }

    public(package) fun new<WriteCap>(_cap: &mut WriteCap, ctx: &mut TxContext): Config<WriteCap> {
        Config<WriteCap> { id: object::new(ctx) }
    }

    #[allow(lint(share_owned))]
    public(package) fun create<WriteCap>(cap: &mut WriteCap, ctx: &mut TxContext) {
        transfer::share_object(new<WriteCap>(cap, ctx))
    }

    public(package) fun new_for_epoch<
        WriteCap,
        Name: copy + drop + store,
        Value: copy + drop + store,
    >(
        config: &mut Config<WriteCap>,
        _cap: &mut WriteCap,
        name: Name,
        value: Value,
        ctx: &mut TxContext,
    ): Option<Value> {
        let epoch = ctx.epoch();
        if (!field::exists_(&config.id, name)) {
            let sobj = Setting {
                id: object::new(ctx),
                data: option::some(SettingData {
                    newer_value_epoch: epoch,
                    newer_value: value,
                    older_value_opt: option::none(),
                }),
            };
            field::add(&mut config.id, name, sobj);
            option::none()
        } else {
            let sobj: &mut Setting<Value> = field::borrow_mut(&mut config.id, name);
            let SettingData {
                newer_value_epoch,
                newer_value,
                older_value_opt,
            } = sobj.data.extract();
            assert!(epoch > newer_value_epoch, EAlreadySetForEpoch);
            sobj.data.fill(SettingData {
                newer_value_epoch: epoch,
                newer_value: value,
                older_value_opt: option::some(newer_value),
            });
            older_value_opt
        }
    }

    public(package) fun exists_with_type<
        WriteCap,
        Name: copy + drop + store,
        Value: copy + drop + store,
    >(
        config: &Config<WriteCap>,
        name: Name,
    ): bool {
        field::exists_with_type<_, Value>(&config.id, name)
    }

    #[allow(unused_mut_parameter)]
    public(package) fun exists_with_type_for_epoch<
        WriteCap,
        Name: copy + drop + store,
        Value: copy + drop + store,
    >(
        config: & Config<WriteCap>,
        name: Name,
        ctx: &TxContext,
    ): bool {
        field::exists_with_type<_, Value>(&config.id, name) && {
            let epoch = ctx.epoch();
            let sobj: &Setting<Value> = field::borrow(&config.id, name);
            epoch == sobj.data.borrow().newer_value_epoch
        }
    }

    #[allow(unused_mut_parameter)]
    public(package) fun borrow_for_epoch_mut<
        WriteCap,
        Name: copy + drop + store,
        Value: copy + drop + store,
    >(
        config: &mut Config<WriteCap>,
        _cap: &mut WriteCap,
        name: Name,
        ctx: &mut TxContext,
    ): &mut Value {
        let epoch = ctx.epoch();
        let sobj: &mut Setting<Value> = field::borrow_mut(&mut config.id, name);
        let data = sobj.data.borrow_mut();
        assert!(data.newer_value_epoch == epoch, ENotSetForEpoch);
        &mut data.newer_value
    }

    public(package) fun borrow_most_recent<
        WriteCap,
        Name: copy + drop + store,
        Value: copy + drop + store,
    >(
        config: &Config<WriteCap>,
        name: Name,
    ): &Value {
        let sobj: &Setting<Value> = field::borrow(&config.id, name);
        &sobj.data.borrow().newer_value
    }

    public(package) macro fun entry<
        $WriteCap,
        $Name: copy + drop + store,
        $Value: copy + drop + store,
    >(
        $config: &mut Config<$WriteCap>,
        $cap: &mut $WriteCap,
        $name: $Name,
        $initial_for_next_epoch: |&mut Config<$WriteCap>, &mut $WriteCap, &mut TxContext| -> $Value,
        $ctx: &mut TxContext,
    ): &mut $Value {
        let config = $config;
        let cap = $cap;
        let name = $name;
        let ctx = $ctx;
        if (!config.exists_with_type_for_epoch<_, _, $Value>(name, ctx)) {
            let initial = $initial_for_next_epoch(config, cap, ctx);
            config.new_for_epoch(cap, name, initial, ctx);
        };
        config.borrow_for_epoch_mut(cap, name, ctx)
    }

    public(package) macro fun update<
        $WriteCap,
        $Name: copy + drop + store,
        $Value: copy + drop + store,
    >(
        $config: &mut Config<$WriteCap>,
        $cap: &mut $WriteCap,
        $name: $Name,
        $initial_for_next_epoch: |&mut Config<$WriteCap>, &mut $WriteCap, &mut TxContext| -> $Value,
        $update_for_next_epoch: |Option<$Value>, &mut $Value|,
        $ctx: &mut TxContext,
    ) {
        let config = $config;
        let cap = $cap;
        let name = $name;
        let ctx = $ctx;
        let old_value_opt = if (!config.exists_with_type_for_epoch<_, _, $Value>(name, ctx)) {
            let initial = $initial_for_next_epoch(config, cap, ctx);
            config.new_for_epoch(cap, name, initial, ctx)
        } else {
            option::none()
        };
        $update_for_next_epoch(old_value_opt, config.borrow_for_epoch_mut(cap, name, ctx));
    }

    public(package) fun read_setting<Name: copy + drop + store, Value: copy + drop + store>(
        config: ID,
        name: Name,
        ctx: &TxContext,
    ): Option<Value> {
        let config_id = config.to_address();
        let setting_df = field::hash_type_and_key(config_id, name);
        read_setting_impl<
            Name,
            Setting<Value>,
            SettingData<Value>,
            Value,
        >(config_id, setting_df, ctx.epoch())
    }

    /*
    This is kept native to keep gas costing consistent.
    */
    native fun read_setting_impl<
        Name: copy + drop + store,
        SettingValue: key + store,
        SettingDataValue: store,
        Value: copy + drop + store,
    >(
        config: address,
        name: address,
        current_epoch: u64,
    ): Option<Value>;
        /*
    // but the code is essentially
        if (!field::exists_with_type<Name, Value>(&config.id, setting)) return option::none()
        let sobj: &Setting<Value> = field::borrow(&config.id, setting);
        let data = sobj.data.borrow();
        if (current_epoch > data.newer_value_epoch) option::some(data.newer_value)
        else data.older_value_opt

    }
    */

}
