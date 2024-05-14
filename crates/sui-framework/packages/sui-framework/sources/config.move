
module sui::config {

    use sui::dynamic_field as field;

    // #[error]
    // const EInvalidWriteCap: vector<u8> = b"WriteCap is not valid for the provided Config";
    const EInvalidWriteCap: u64 = 0;

    // #[error]
    // const EAlreadySetForEpoch: vector<u8> =
    //     b"Setting was already updated at this epoch for the provided Config";
    const EAlreadySetForEpoch: u64 = 1;

    // #[error]
    // const ENotSetForEpoch: vector<u8> =
    //     b"Setting was not updated at this epoch for the provided Config";
    const ENotSetForEpoch: u64 = 2;

    // #[error]
    // const ENotSetForEpoch: vector<u8> = b"Could not read setting for the provided Config";
    const EReadSettingFailed: u64 = 3;

    public struct Config has key {
        id: UID,
    }

    public struct WriteCap has key, store {
        id: UID,
        config: ID,
    }

    public struct Setting<V: store> has key, store {
        id: UID,
        data: Option<SettingData<V>>,
    }


    public struct SettingData<V: store> has store {
        newer_value_epoch: u64,
        newer_value: V,
        older_value_opt: Option<V>,
    }

    public fun create_config(ctx: &mut TxContext): (Config, WriteCap) {
        let config = Config { id: object::new(ctx) };
        let cap = WriteCap {
            id: object::new(ctx),
            config: config.id.to_inner(),
        };
        (config, cap)
    }

    public fun new_for_epoch<K: copy + drop + store, V: store>(
        cap: &mut WriteCap,
        config: &mut Config,
        setting: K,
        value: V,
        ctx: &mut TxContext,
    ): Option<V> {
        assert!(cap.config == config.id.to_inner(), EInvalidWriteCap);
        let epoch = ctx.epoch();
        if (!field::exists_(&config.id, setting)) {
            let sobj = Setting {
                id: object::new(ctx),
                data: option::some(SettingData {
                    newer_value_epoch: epoch,
                    newer_value: value,
                    older_value_opt: option::none(),
                }),
            };
            field::add(&mut config.id, setting, sobj);
            option::none()
        } else {
            let sobj: &mut Setting<V> = field::borrow_mut(&mut config.id, setting);
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

    public fun has_for_epoch<K: copy + drop + store, V: store>(
        config: &mut Config,
        setting: K,
        ctx: &mut TxContext,
    ): bool {
        field::exists_(&config.id, setting) && {
            let epoch = ctx.epoch();
            let sobj: &Setting<V> = field::borrow(&config.id, setting);
            epoch == sobj.data.borrow().newer_value_epoch
        }
    }

    // I believe this is safe
    public fun borrow_mut<K: copy + drop + store, V: store>(
        cap: &mut WriteCap,
        config: &mut Config,
        setting: K,
        ctx: &mut TxContext,
    ): &mut V {
        assert!(cap.config == config.id.to_inner(), EInvalidWriteCap);
        let epoch = ctx.epoch();
        let sobj: &mut Setting<V> = field::borrow_mut(&mut config.id, setting);
        let data = sobj.data.borrow_mut();
        assert!(data.newer_value_epoch == epoch, ENotSetForEpoch);
        &mut data.newer_value
    }

    // public macro fun update<K: copy + drop + store, V: store>(
    //     cap: &mut WriteCap,
    //     config: &mut Config,
    //     setting: K,
    //     ctx: &mut TxContext,
    //     default: || V,
    //     update: |&mut V|,
    // ) {
    //     if !has_for_epoch(config, setting, ctx) {
    //         new_for_epoch(cap, config, setting, default(), ctx);
    //     }
    //     let new_value = borrow_mut(cap, config, setting, ctx);
    //     update(new_value);
    // }

    public fun borrow<K: copy + drop + store, V: copy + drop + store>(
        config: ID,
        setting: K,
        ctx: &TxContext,
    ): &V {
        let config_id = config.to_address();
        let setting_df = field::hash_type_and_key(config_id, setting);
        borrow_setting_<K, V>(config_id, setting_df, ctx.epoch())
    }

    /*
    This is kept native to keep gas costing consistent.
    */
    native fun borrow_setting_<K: copy + drop + store, V: store>(
        config: address,
        setting: address,
        current_epoch: u64,
    ): &V;
        /*
    // but the code is essentially
        assert!(field::exists_with_type<K, V>(&config.id, setting), EReadSettingFailed);
        let sobj: &Setting<V> = field::borrow(&config.id, setting);
        let data = sobj.data.borrow();
        if (current_epoch > data.newer_value_epoch) &data.newer_value
        else {
            assert!(data.older_value_opt.is_some(), EReadSettingFailed); // internal invariant
            data.older_value_opt.borrow()
        }
    }
    */

}
