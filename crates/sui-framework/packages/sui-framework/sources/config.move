
// module sui::config {

//     use sui::dynamic_field as field;

//     // #[error]
//     // const EInvalidWriteCap: vector<u8> = b"WriteCap is not valid for the provided Config";
//     const EInvalidWriteCap: u64 = 0;

//     public struct Config has key {
//         id: UID,
//     }

//     public struct WriteCap has key, store {
//         id: UID,
//         config: ID,
//     }

//     public struct SettingData<V: store> {
//         newer_value_epoch: u64,
//         newer_value: V,
//         older_value_opt: Option<V>,
//     }

//     public struct Setting<V: store> has key, store {
//         id: UID,
//         data: Option<SettingData<V>>,
//     }

//     public fun create_config(ctx: &mut TxContext): (Config, WriteCap) {
//         let config = Config { id: object::new(ctx) };
//         let cap = WriteCap {
//             id: object::new(ctx),
//             config: config.id.to_inner(),
//         };
//         (config, cap)
//     }

//     public fun new_for_epoch<K: copy + drop + store, V: store>(
//         cap: &mut WriteCap,
//         config: &mut Config,
//         setting: K,
//         value: V,
//         ctx: &mut TxContext,
//     ): Option<V> {
//         assert!(cap.config == config.id.to_inner(), EInvalidWriteCap);
//         let epoch = ctx.epoch();
//         if (!field::exists_(&config.id, setting)) {
//             let sobj = Setting {
//                 id: object::new(ctx),
//                 data: option::some(SettingData {
//                     newer_value_epoch: epoch,
//                     newer_value: value,
//                     older_value_opt: option::none(),
//                 }),
//             };
//             field::add(&mut config.id, setting, sobj);
//             option::none()
//         } else {
//             let sobj: &mut Setting<V> = field::borrow_mut(&mut config.id, setting);
//             let SettingData {
//                 newer_value_epoch,
//                 newer_value,
//                 older_value_opt,
//             } = sobj.data.extract();
//             assert!(epoch > newer_value_epoch, EAlreadySetForEpoch);
//             let older_value = sobj.older_value;
//             sobj.fill(option::some(SettingData {
//                 newer_value_epoch: epoch,
//                 newer_value: value,
//                 older_value_opt: option::some(newer_value),
//             }));
//             older_value_opt
//         }
//     }

//     public fun has_for_epoch<K: copy + drop + store, V: store>(
//         config: &mut Config,
//         setting: K,
//         ctx: &mut TxContext,
//     ): bool {
//         df::exists_(&config.id) && {
//             let epoch = ctx.epoch();
//             let Setting {
//                 older_value,
//                 older_value_epoch,
//                 newer_value,
//                 newer_value_epoch,
//             } = df::borrow(&config.id, setting);
//             epoch == newer_value_epoch
//             true
//         }
//     }

//     // I believe this is safe
//     public fun borrow_mut<K: copy + drop + store, V: store>(
//         cap: &mut WriteCap,
//         config: &mut Config,
//         setting: K,
//         ctx: &mut TxContext,
//     ): &mut V {
//         assert!(cap.id == config.id.to_inner(), EInvalidWriteCap);
//         let epoch = ctx.epoch();
//         let setting_field = df::borrow_mut(&mut config.id, setting)
//         assert!(setting_field.newer_value_epoch == epoch, /* TODO */ 0);
//         &mut setting_field.newer_value
//     }

//     public macro fun update<K: copy + drop + store, V: store>(
//         cap: &mut WriteCap,
//         config: &mut Config,
//         setting: K,
//         ctx: &mut TxContext,
//         default: || V,
//         update: |&mut V|,
//     ) {
//         if !has_for_epoch(config, setting, ctx) {
//             new_for_epoch(cap, config, setting, default(), ctx);
//         }
//         let new_value = borrow_mut(cap, config, setting, ctx);
//         update(new_value);
//     }

//     public fun borrow<K: copy + drop + store, V: copy + drop + store>(
//         config: ID,
//         setting: K,
//         ctx: &TxContext,
//     ) {
//         let object_addr = object::id_to_address(config);
//         // public(friend)
//         let hash = df::hash_type_and_key(object_addr, name);
//         read_setting_<K, V>(hash, setting, ctx.epoch())
//     }

//     /*
//     This is kept native to keep gas costing consistent.
//     */
//     native fun read_setting_<K: copy + drop + store, V: store>(
//         config_id: address,
//         setting_id: address,
//         current_epoch: u64,
//     ): &V;
//     /*
//     but the code is essentially
//         assert!(df::exists_with_type<K, V>(config), EReadSettingFailed);
//         let Setting {
//             older_value,
//             older_value_epoch,
//             newer_value,
//             newer_value_epoch,
//         } = df::borrow(config, setting);
//         if (current_epoch > newer_value_epoch) newer_value
//         else {
//             assert!(option::is_some(older_value), EReadSettingFailed);
//             option::borrow(older_value)
//         }
//     */

// }
