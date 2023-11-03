// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The Capy module. Defines the Capy type and its functions.
module capy::capy {
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID, ID};
    use std::string::{Self, String};
    use sui::url::{Self, Url};
    use sui::transfer;
    use sui::event::emit;
    use sui::dynamic_object_field as dof;

    use std::vector as vec;
    use std::option::{Self, Option};
    use std::hash::sha3_256 as hash;

    friend capy::capy_winter;

    /// Number of meaningful genes. Also marks the length
    /// of the hash used in the application: sha3_256.
    const GENES: u64 = 32;

    /// There's a chance to apply mutation on each gene.
    /// For testing reasons mutation chance is relatively high: 5/255.
    const MUTATION_CHANCE: u8 = 250;

    /// Base path for `Capy.url` attribute. Is temporary and improves
    /// explorer / wallet display. Always points to the dev/testnet server.
    const IMAGE_URL: vector<u8> = b"https://api.capy.art/capys/";

    /// Link to the capy on the capy.art.
    const MAIN_URL: vector<u8> = b"https://capy.art/capy/";

    // ======== Types =========

    /// Internal representation of a Gene Sequence. Each Gene
    /// corresponds to an attribute. Max number of genes is `GENES`.
    struct Genes has store, copy, drop { sequence: vector<u8> }

    /// Defines a Capy attribute. Eg: `pattern: 'panda'`
    struct Attribute has store, copy, drop {
        name: String,
        value: String,
    }

    /// One of the possible values defined in GeneDefinition.
    /// `selector` field corresponds to the u8 value of the gene in a sequence.
    ///
    /// See `breed` function for details on usage.
    struct Value has store, drop, copy {
        selector: u8,
        name: String
    }

    /// Holds the definitions for each gene. They are then assigned to
    /// Capys. Newborn will receive attributes available at the time.
    struct GeneDefinition has store {
        name: String,
        values: vector<Value>
    }

    /// The Capy itself. Every Capy has its unique set of genes,
    /// as well as generation and parents. Ownable, tradeable.
    struct Capy has key, store {
        id: UID,
        gen: u32,
        url: Url,
        link: Url,
        genes: Genes,
        dev_genes: Genes,
        item_count: u8,
        attributes: vector<Attribute>,
    }

    /// Belongs to the creator of the game. Has store, which
    /// allows building something on top of it (ie shared object with
    /// multi-access policy for managers).
    struct CapyManagerCap has key, store { id: UID }

    /// Every capybara is registered here. Acts as a source of randomness
    /// as well as the storage for the main information about the gamestate.
    struct CapyRegistry has key {
        id: UID,
        capy_born: u64,
        capy_hash: vector<u8>,
        genes: vector<GeneDefinition>
    }


    // ======== Events =========

    /// Event. When a new registry has been created.
    /// Marks the start of the game.
    struct RegistryCreated has copy, drop { id: ID }

    /// Event. Emitted when a new Gene definition was added.
    /// Helps announcing new features.
    struct GeneDefinitionAdded has copy, drop {
        name: String,
        values: vector<Value>
    }

    /// Event. When new Capy is born.
    struct CapyBorn has copy, drop {
        id: ID,
        gen: u32,
        genes: Genes,
        dev_genes: Genes,
        attributes: vector<Attribute>,
        parent_one: Option<ID>,
        parent_two: Option<ID>,
        bred_by: address
    }

    /// Event. Emitted when a new item is added to a capy.
    struct ItemAdded<phantom T> has copy, drop {
        capy_id: ID,
        item_id: ID
    }

    /// Event. Emitted when an item is taken off.
    struct ItemRemoved<phantom T> has copy, drop {
        capy_id: ID,
        item_id: ID,
    }

    // ======== View Functions ========

    /// Read extra gene sequence of a Capy as `vector<u8>`.
    public fun dev_genes(self: &Capy): &vector<u8> {
        &self.dev_genes.sequence
    }

    // ======== Functions =========

    #[allow(unused_function)]
    /// Create a shared CapyRegistry and give its creator the capability
    /// to manage the game.
    fun init(ctx: &mut TxContext) {
        let id = object::new(ctx);
        let capy_hash = hash(object::uid_to_bytes(&id));

        emit(RegistryCreated { id: object::uid_to_inner(&id) });

        transfer::public_transfer(CapyManagerCap { id: object::new(ctx) }, tx_context::sender(ctx));
        transfer::share_object(CapyRegistry {
            id,
            capy_hash,
            capy_born: 0,
            genes: vec::empty()
        })
    }


    // ======= Admin Functions =======

    /// This method is rather complicated.
    /// To define a new set of attributes, Admin must send it in a format:
    /// ```
    /// name = b"name of the attribute"
    /// definitions = [
    ///     [selector_u8, ...name_bytes],
    ///     [selector_u8, ...name_bytes]
    /// ]
    /// ```
    public entry fun add_gene(
        _: &CapyManagerCap,
        reg: &mut CapyRegistry,
        name: vector<u8>,
        definitions: vector<vector<u8>>,
        _ctx: &mut TxContext
    ) {
        let name = string::utf8(name);
        let values = raw_vec_to_values(definitions);

        // emit an event confirming gene addition
        emit(GeneDefinitionAdded { name: *&name, values: *&values });

        // lastly add new gene definition to the registry
        vec::push_back(&mut reg.genes, GeneDefinition { name, values });
    }

    /// Batch-add new Capys with predefined gene sequences.
    public fun batch(_: &CapyManagerCap, reg: &mut CapyRegistry, genes: vector<vector<u8>>, ctx: &mut TxContext): vector<Capy> {
        let capys = vector[];
        while (vec::length(&genes) > 0) {
            let sequence = vec::pop_back(&mut genes);
            let capy = create_capy(reg, sequence, vector[], ctx);

            vec::push_back(&mut capys, capy)
        };

        capys
    }

    /// Creates an attribute with the given name and a value. Should only be used for
    /// events. Is currently a friend-only feature but will be put behind a capability
    /// authorization later.
    public(friend) fun create_attribute(name: vector<u8>, value: vector<u8>): Attribute {
        Attribute {
            name: string::utf8(name),
            value: string::utf8(value)
        }
    }

    /// Create a Capy with a specified gene sequence.
    /// Also allows assigning custom attributes if an App is authorized to do it.
    public(friend) fun create_capy(
        reg: &mut CapyRegistry, sequence: vector<u8>, custom_attributes: vector<Attribute>, ctx: &mut TxContext
    ): Capy {
        let id = object::new(ctx);
        let genes = Genes { sequence };
        let dev_genes = Genes { sequence: hash(sequence) };

        reg.capy_born = reg.capy_born + 1;

        vec::append(&mut reg.capy_hash, object::uid_to_bytes(&id));
        reg.capy_hash = hash(reg.capy_hash);

        let sender = tx_context::sender(ctx);
        let attributes = get_attributes(&reg.genes, &genes);

        vec::append(&mut attributes, custom_attributes);

        emit(CapyBorn {
            id: object::uid_to_inner(&id),
            gen: 0,
            attributes: *&attributes,
            genes: *&genes,
            dev_genes: *&dev_genes,
            parent_one: option::none(),
            parent_two: option::none(),
            bred_by: sender
        });

        Capy {
            url: img_url(&id),
            link: link_url(&id),
            id,
            genes,
            dev_genes,
            attributes,
            gen: 0,
            item_count: 0,
        }
    }

    // ======= User facing functions =======

    /// Attach an Item to a Capy. Function is generic and allows any app to attach items to
    /// Capys but the total count of items has to be lower than 255.
    public entry fun add_item<T: key + store>(capy: &mut Capy, item: T) {
        emit(ItemAdded<T> {
            capy_id: object::id(capy),
            item_id: object::id(&item)
        });

        dof::add(&mut capy.id, object::id(&item), item);
    }

    /// Remove item from the Capy.
    public entry fun remove_item<T: key + store>(capy: &mut Capy, item_id: ID, ctx: &TxContext) {
        emit(ItemRemoved<T> {
            capy_id: object::id(capy),
            item_id: *&item_id
        });

        transfer::public_transfer(dof::remove<ID, T>(&mut capy.id, item_id), tx_context::sender(ctx));
    }

    /// Breed capys and keep the newborn at sender's address.
    public entry fun breed_and_keep(
        reg: &mut CapyRegistry,
        c1: &mut Capy,
        c2: &mut Capy,
        ctx: &mut TxContext
    ) {
        transfer::public_transfer(breed(reg, c1, c2, ctx), tx_context::sender(ctx))
    }

    /// Breed two Capys together. Perform a gene science algorithm and select
    /// genes for the newborn based on the parents' genes.
    public fun breed(
        reg: &mut CapyRegistry,
        c1: &mut Capy,
        c2: &mut Capy,
        ctx: &mut TxContext
    ): Capy {
        let id = object::new(ctx);

        // Update capy hash in the registry
        vec::append(&mut reg.capy_hash, object::uid_to_bytes(&id));

        // compute genes
        reg.capy_hash = hash(reg.capy_hash);
        let genes = compute_genes(&reg.capy_hash, &c1.genes, &c2.genes, GENES);

        // compute dev-genes
        reg.capy_hash = hash(reg.capy_hash);
        let dev_genes = compute_genes(&reg.capy_hash, &c1.genes, &c2.genes, GENES);

        let gen = if (c1.gen > c2.gen) { c1.gen } else { c2.gen } + 1;
        let attributes = get_attributes(&reg.genes, &genes);
        let sender = tx_context::sender(ctx);

        emit(CapyBorn {
            id: object::uid_to_inner(&id),
            gen,
            genes: *&genes,
            attributes: *&attributes,
            dev_genes: *&dev_genes,
            parent_one: option::some(object::id(c1)),
            parent_two: option::some(object::id(c2)),
            bred_by: sender
        });

        // Send newborn to parents.
        Capy {
            url: img_url(&id),
            link: link_url(&id),
            id,
            gen,
            genes,
            dev_genes,
            attributes,
            item_count: 0,
        }
    }

    // ======= Private and Utility functions =======

    /// Get Capy attributes from the gene sequence.
    fun get_attributes(definitions: &vector<GeneDefinition>, genes: &Genes): vector<Attribute> {
        let attributes = vec::empty();
        let (i, len) = (0u64, vec::length(definitions));
        while (i < len) {
            let gene_def = vec::borrow(definitions, i);
            let capy_gene = vec::borrow(&genes.sequence, i);

            let (j, num_options) = (0u64, vec::length(&gene_def.values));
            while (j < num_options) {
                let value = vec::borrow(&gene_def.values, j);
                if (*capy_gene <= value.selector) {
                    vec::push_back(&mut attributes, Attribute {
                        name: *&gene_def.name,
                        value: *&value.name
                    });
                    break
                };
                j = j + 1;
            };
            i = i + 1;
        };

        attributes
    }

    /// Computes genes for the newborn based on the random seed r0, and parents genes
    /// The `max` parameter affects how many genes should be changed (if there are no
    /// attributes yet for the)
    fun compute_genes(r0: &vector<u8>, g1: &Genes, g2: &Genes, max: u64): Genes {
        let i = 0;

        let s1 = &g1.sequence;
        let s2 = &g2.sequence;
        let s3 = vec::empty();

        let r1 = derive(r0, 1); // for parent gene selection
        let r2 = derive(r0, 2); // chance of random mutation
        let r3 = derive(r0, 3); // value selector for random mutation

        while (i < max) {
            let rng = *vec::borrow(&r1, i);
            let gene = if (lor(rng, 127)) {
                *vec::borrow(s1, i)
            } else {
                *vec::borrow(s2, i)
            };

            // There's a tiny chance that a mutation will happen.
            if (lor(*vec::borrow(&r2, i), MUTATION_CHANCE)) {
                gene = *vec::borrow(&r3, i);
            };

            vec::push_back(&mut s3, gene);
            i = i + 1;
        };

        Genes { sequence: s3 }
    }

    /// Give true or false based on the number.
    /// Used for selecting mother/father genes.
    fun lor(rng: u8, cmp: u8): bool {
        (rng > cmp)
    }

    /// Derive something from the seed. Add a derivation path as u8, and
    /// hash the result.
    fun derive(r0: &vector<u8>, path: u8): vector<u8> {
        let r1 = *r0;
        vec::push_back(&mut r1, path);
        hash(r1)
    }


    // ==== Utilities ======

    /// Transforms a vector of raw definitions:
    /// [
    ///    [127, b"red"],
    ///    [255, b"blue"],
    /// ]
    /// Into a vector of `Value`s (in order!):
    /// [
    ///    Value { selector: 127, name: String("red") },
    ///    Value { selector: 255, name: String("blue") },
    /// ]
    fun raw_vec_to_values(definitions: vector<vector<u8>>): vector<Value> {
        let result = vec::empty();
        vec::reverse(&mut definitions);
        while (vec::length(&definitions) > 0) {
            // [selector, name]
            let value_def = vec::pop_back(&mut definitions);
            // [eman, selector]
            vec::reverse(&mut value_def);
            let selector = vec::pop_back(&mut value_def);
            let name = vec::empty();
            while (vec::length(&value_def) > 0) {
                vec::push_back(&mut name, vec::pop_back(&mut value_def));
            };

            vec::push_back(&mut result, Value {
                selector,
                name: string::utf8(name)
            });
        };

        result
    }

    /// Construct an image URL for the capy.
    fun img_url(c: &UID): Url {
        let capy_url = *&IMAGE_URL;
        vec::append(&mut capy_url, sui::hex::encode(object::uid_to_bytes(c)));
        vec::append(&mut capy_url, b"/svg");

        url::new_unsafe_from_bytes(capy_url)
    }

    /// Construct a Url to the capy.art.
    fun link_url(c: &UID): Url {
        let capy_url = *&MAIN_URL;
        vec::append(&mut capy_url, sui::hex::encode(object::uid_to_bytes(c)));
        url::new_unsafe_from_bytes(capy_url)
    }

    #[test]
    fun test_raw_vec_to_values() {
        let definitions: vector<vector<u8>> = vec::empty();

        /* push [127, "red"] */ {
            let def = vec::empty();
            vec::push_back(&mut def, 127);
            vec::append(&mut def, b"red");
            vec::push_back(&mut definitions, def);
        };

        /* push [255, "blue"] */ {
            let def = vec::empty();
            vec::push_back(&mut def, 255);
            vec::append(&mut def, b"blue");
            vec::push_back(&mut definitions, def);
        };

        let values: vector<Value> = raw_vec_to_values(definitions);

        /* expect [255, blue] */ {
            let Value { selector, name } = vec::pop_back(&mut values);
            assert!(selector == 255, 0);
            assert!(string::bytes(&name) == &b"blue", 0);
        };

        /* expect [127, red] */ {
            let Value { selector, name } = vec::pop_back(&mut values);
            assert!(selector == 127, 0);
            assert!(string::bytes(&name) == &b"red", 0);
        };

        vec::destroy_empty(values);
    }
}
