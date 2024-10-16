# Move Paketlerinde Hata Ayıklayın ve Yayınlayın

### Bir pakette hata ayıklama <a href="#debugging-a-package" id="debugging-a-package"></a>

Şu anda Move için henüz bir hata ayıklayıcı yoktur. Ancak hata ayıklamaya yardımcı olması için `std::debug` modülünü kullanarak rastgele bir değer yazdırabilirsiniz. Bunu yapmak için önce `debug` modülünü içe aktarın:

```
use std::debug;
```

Ardından, türüne bakılmaksızın bir `v` değerini yazdırmak istediğiniz yerlerde bunu yapmanız yeterlidir:

```
debug::print(&v);
```

ya da v zaten bir referans ise aşağıdakini yapabilirsiniz:

```
debug::print(v);
```

`debug` modülü ayrıca geçerli yığın izini yazdırmak için bir işlev sağlar:

```
debug::print_stack_trace();
```

Alternatif olarak, herhangi bir `abort` çağrısı veya assertion hatası da hata noktasındaki stacktrace'i yazdıracaktır.

> **Important:** Yeni modül yayınlanmadan önce hata ayıklama modülündeki işlevlere yapılan tüm çağrılar test edilmeyen koddan kaldırılmalıdır (test kodu `#[test]` ek açıklamasıyla işaretlenir).

### Paketi yayınlama <a href="#publishing-a-package" id="publishing-a-package"></a>

Bir Move paketindeki işlevlerin Sui'den gerçekten çağrılabilir olması için (Sui yürütme senaryosunun taklit edilmesi yerine), paketin Sui'nin [dağıtılmış defterinde](https://docs.sui.io/devnet/learn/how-sui-works) _yayınlanması_ ve burada bir Sui nesnesi olarak temsil edilmesi gerekir.

Ancak bu noktada, `sui move` komutu paket yayınlamayı desteklememektedir. Aslında, bir birim test çerçevesi bağlamında, paket oluşturma başına bir kez gerçekleşen paket yayınlamayı barındırmanın mantıklı olup olmadığı bile açık değildir. Bunun yerine, Move kodunu [yayınlamak](https://docs.sui.io/devnet/build/cli-client#publish-packages) ve [çağırmak](https://docs.sui.io/devnet/build/cli-client#calling-move-code) için bir [Sui CLI istemcisi](https://docs.sui.io/devnet/build/cli-client) kullanılabilir. Bu eğitimin bir parçası olarak [yazdığımız ](https://docs.sui.io/devnet/build/move/write-package)paketin nasıl yayınlanacağının açıklaması için Sui CLI istemci belgelerine bakın.

#### Modül başlatıcıları <a href="#module-initializers" id="module-initializers"></a>

Bununla birlikte, Sui'de Move kodu geliştirmeyi etkileyen paket yayınlamanın önemli bir yönü vardır - bir paketteki her modül, yayınlama zamanında çalıştırılacak özel bir _başlatıcı işlevi_ içerebilir. Bir başlatıcı işlevin amacı, modüle özgü verileri önceden başlatmaktır (örneğin, singleton nesneleri oluşturmak). Başlatıcı fonksiyonun yayın sırasında çalıştırılabilmesi için aşağıdaki özelliklere sahip olması gerekir:

* isim `init`
* `&mut TxContext` türünün tek parametresi
* dönüş değeri yok
* özel görünürlük

`sui move` komutu açıkça yayınlamayı desteklemese de, test çerçevemizi kullanarak modül başlatıcılarını test edebiliriz - ilk işlemi başlatıcı işlevini yürütmeye ayırabiliriz. Bunu göstermek için somut bir örnek kullanalım.

Fantezi oyun örneğimize devam ederek, kılıç yaratma sürecine dahil olacak bir demirci kavramını tanıtalım - yeni başlayanlar için kaç kılıç yaratıldığını takip etsin. `Forge` struct'ını ve yaratılan kılıç sayısını döndüren bir fonksiyonu aşağıdaki gibi tanımlayalım ve `my_module.move` dosyasına koyalım:

```
    struct Forge has key, store {
        id: UID,
        swords_created: u64,
    }

    public fun swords_created(self: &Forge): u64 {
        self.swords_created
    }
```

Oluşturulan kılıçların sayısını takip etmek için forge nesnesini başlatmalı ve `sword_create` sayısını 0 olarak ayarlamalıyız. Ve modül başlatıcı bunu yapmak için mükemmel bir yerdir:

```
    // module initializer to be executed when this module is published
    fun init(ctx: &mut TxContext) {
        use sui::transfer;
        use sui::tx_context;
        let admin = Forge {
            id: object::new(ctx),
            swords_created: 0,
        };
        // transfer the forge object to the module/package publisher
        // (presumably the game admin)
        transfer::transfer(admin, tx_context::sender(ctx));
    }
```

Demirciliği kullanmak için, `sword_create` fonksiyonunu demirciliği parametre olarak alacak ve fonksiyonun sonunda oluşturulan kılıç sayısını güncelleyecek şekilde değiştirmemiz gerekir:

```
    public entry fun sword_create(forge: &mut Forge, magic: u64, strength: u64, recipient: address, ctx: &mut TxContext) {
        ...
        forge.swords_created = forge.swords_created + 1;
    }
```

Şimdi modülün başlatılmasını test etmek için bir fonksiyon oluşturabiliriz:

```
    #[test]
    public fun test_module_init() {
        use sui::test_scenario;

        // create test address representing game admin
        let admin = @0xBABE;

        // first transaction to emulate module initialization
        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario));
        };
        // second transaction to check if the forge has been created
        // and has initial value of zero swords created
        test_scenario::next_tx(scenario, admin);
        {
            // extract the Forge object
            let forge = test_scenario::take_from_sender<Forge>(scenario);
            // verify number of created swords
            assert!(swords_created(&forge) == 0, 1);
            // return the Forge object to the object pool
            test_scenario::return_to_sender(scenario, forge);
        };
        test_scenario::end(scenario_val);
    }

```

Yukarıda tanımlanan test fonksiyonunda görebileceğimiz gibi, ilk işlemde (açıkça) başlatıcıyı çağırıyoruz ve bir sonraki işlemde forge nesnesinin oluşturulup oluşturulmadığını ve düzgün bir şekilde başlatılıp başlatılmadığını kontrol ediyoruz.

Bu noktada tüm paket üzerinde testleri çalıştırmaya çalışırsak, `sword_create` fonksiyon imzası değişikliği nedeniyle mevcut testlerde derleme hatalarıyla karşılaşırız. Testlerin tekrar çalışabilmesi için gereken değişiklikleri okuyucuya bir alıştırma olarak bırakacağız. Geliştirdiğimiz paketin tüm kaynak kodunu (tüm testler düzgün bir şekilde ayarlanmış olarak) [my\_module.move](https://github.com/MystenLabs/sui/tree/main/sui\_programmability/examples/move\_tutorial/sources/my\_module.move) dosyasında bulabilirsiniz.
