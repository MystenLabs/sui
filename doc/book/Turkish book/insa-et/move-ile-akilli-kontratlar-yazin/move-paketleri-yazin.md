# Move Paketleri Yazın

Bir Move paketi oluşturmak ve bu pakette tanımlanan kodu çalıştırmak için önce Sui binary'lerini yükleyin.

#### Paketi oluşturma <a href="#creating-the-package" id="creating-the-package"></a>

İlk olarak boş bir Move paketi oluşturun:

```
$ sui move new my_first_package
```

Bu, `my_first_package` dizininde bir iskelet Move projesi oluşturur. Şimdi bu komut tarafından oluşturulan paket manifestine bir göz atalım:

```
$ cat my_first_package/Move.toml
[package]
name = "my_first_package"
version = "0.0.1"

[dependencies]
Sui = { git = "https://github.com/MystenLabs/sui.git", subdir = "crates/sui-framework", rev = "devnet" }

[addresses]
my_first_package = "0x0"
sui = "0x2"
```

Bu dosya şunları içerir:

* İsim ve sürüm gibi paket meta verileri (`[package]` bölümü)
* Bu paketin bağlı olduğu diğer paketler (`[dependencies]` bölümü). Bu paket yalnızca Sui Framework'e bağlıdır, ancak diğer üçüncü taraf bağımlılıkları buraya eklenmelidir.
* Adlandırılmış _adreslerin_ bir listesi (`[addresses]` bölümü). Bu isimler, kaynak kodda verilen adresler için uygun takma adlar olarak kullanılabilir.

#### Paketi tanımlama <a href="#defining-the-package" id="defining-the-package"></a>

Paket içinde bir kaynak dosya oluşturarak başlayalım:

```
$ touch my_first_package/sources/my_module.move
```

ve `my_module.move` dosyasına aşağıdaki kodu ekleyelim:

```
module my_first_package::my_module {
    // Part 1: imports
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    // Part 2: struct definitions
    struct Sword has key, store {
        id: UID,
        magic: u64,
        strength: u64,
    }

    struct Forge has key, store {
        id: UID,
        swords_created: u64,
    }

    // Part 3: module initializer to be executed when this module is published
    fun init(ctx: &mut TxContext) {
        let admin = Forge {
            id: object::new(ctx),
            swords_created: 0,
        };
        // transfer the forge object to the module/package publisher
        transfer::transfer(admin, tx_context::sender(ctx));
    }

    // Part 4: accessors required to read the struct attributes
    public fun magic(self: &Sword): u64 {
        self.magic
    }

    public fun strength(self: &Sword): u64 {
        self.strength
    }

    public fun swords_created(self: &Forge): u64 {
        self.swords_created
    }

    // part 5: public/ entry functions (introduced later in the tutorial)
    // part 6: private functions (if any)
}
```

Şimdi bu kodun dört farklı bölümünü inceleyelim:

1. İçe aktarmalar: bunlar modülümüzün diğer modüllerde bildirilen türleri ve işlevleri kullanmasını sağlar. Bu durumda, üç farklı modülden içe aktarım yapıyoruz.
2. Struct bildirimleri: bunlar bu modül tarafından oluşturulabilecek/yok edilebilecek tipleri tanımlar. Burada `anahtar` _yetenekler_, bu yapıların adresler arasında aktarılabilen Sui nesneleri olduğunu gösterir. Kılıç üzerindeki `depolama` yeteneği, diğer yapıların alanlarında görünmesini ve serbestçe aktarılmasını sağlar.
3. Modül başlatıcı: Bu, modül yayınlandığında tam olarak bir kez çağrılan özel bir işlevdir.
4. Accessor fonksiyonları--bunlar modülün struct alanlarının diğer modüllerden okunmasını sağlar.
