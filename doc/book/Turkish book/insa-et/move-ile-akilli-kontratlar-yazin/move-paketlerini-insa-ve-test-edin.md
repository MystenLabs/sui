# Move Paketlerini İnşa ve Test Edin

### Paket oluşturma <a href="#building-a-package" id="building-a-package"></a>

Paketinizi içeren `my_move_package` dizininde olduğunuzdan emin olun ve ardından paketi oluşturmak için aşağıdaki komutu kullanın:

<pre><code><strong>$ sui move build
</strong></code></pre>

Başarılı bir build, aşağıdakine benzer bir yanıt döndürür:

```
Build Successful
Artifacts path: "./build"
```

Build başarısız olursa, kök sorunları gidermek ve çözmek için çıktıdaki ayrıntılı hata mesajlarını kullanabilirsiniz.

Varlığımızı ve onun accessor fonksiyonlarını tasarladığımıza göre şimdi yazdığımız kodu test edelim.

### Kodu test etme <a href="#testing-a-package" id="testing-a-package"></a>

Sui, Move kodunu test etmek için diğer diller için test çerçevelerine (örneğin, yerleşik [Rust test ](https://doc.rust-lang.org/rust-by-example/testing/unit\_testing.html)[framework'ü](https://doc.rust-lang.org/rust-by-example/testing/unit\_testing.html) veya Java için [JUnit framework'ü](https://junit.org/)) benzer şekilde birim testleri yazmanıza olanak tanıyan [Move test framework'ü](https://doc.rust-lang.org/rust-by-example/testing/unit\_testing.html) için destek içerir.

Tek bir Move birim testi, parametresi ve geri dönüş değeri olmayan ve #`[test]` ek açıklamasına sahip genel bir işlevde kapsüllenir. Bu tür fonksiyonlar, aşağıdaki komutun çalıştırılması üzerine test çerçevesi tarafından yürütülür (çalışan örneğimize göre `my_move_package` dizininde):

<pre><code><strong>$ sui move test
</strong></code></pre>

Bu komutu [write a package](https://docs.sui.io/devnet/build/move/write-package) içinde oluşturulan paket için çalıştırırsanız, şaşırtıcı olmayan bir şekilde, henüz hiç test yazmadığımız için hiçbir testin çalıştırılmadığını gösteren aşağıdaki çıktıyı göreceksiniz!

```
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyFirstPackage
Running Move unit tests
Test result: OK. Total tests: 0; passed: 0; failed: 0
```

Basit bir test fonksiyonu yazalım ve `my_module.move` dosyasına ekleyelim:

```
    #[test]
    public fun test_sword_create() {
        use sui::tx_context;

        // create a dummy TxContext for testing
        let ctx = tx_context::dummy();

        // create a sword
        let sword = Sword {
            id: object::new(&mut ctx),
            magic: 42,
            strength: 7,
        };

        // check if accessor functions return correct values
        assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
    }
```

Birim test fonksiyonunun kodu büyük ölçüde kendini açıklayıcıdır - kılıç nesnemizin benzersiz bir tanımlayıcısını oluşturmak için gereken `TxContext` struct'ının sahte bir örneğini oluşturuyoruz, ardından kılıcın kendisini oluşturuyoruz ve son olarak doğru değerleri döndürdüklerini doğrulamak için accessor fonksiyonlarını çağırıyoruz. Sahte bağlamın `object::new` fonksiyonuna değiştirilebilir bir referans argümanı (`&mut`) olarak aktarıldığını ve kılıcın kendisinin de accessor fonksiyonlarına salt okunur bir referans argümanı olarak aktarıldığını unutmayın.

Şimdi bir test yazdığımıza göre, testleri tekrar çalıştırmayı deneyelim:

<pre><code><strong>$ sui move test
</strong></code></pre>

Ancak test komutunu çalıştırdıktan sonra, test sonucu yerine bir build hatası alıyoruz:

```
error[E06001]: unused value without 'drop'
   ┌─ ./sources/my_module.move:60:65
   │
 4 │       struct Sword has key, store {
   │              ----- To satisfy the constraint, the 'drop' ability would need to be added here
   ·
27 │           let sword = Sword {
   │               ----- The local variable 'sword' still contains a value. The value does not have the 'drop' ability and must be consumed before the function returns
   │ ╭─────────────────────'
28 │ │             id: object::new(&mut ctx),
29 │ │             magic: 42,
30 │ │             strength: 7,
31 │ │         };
   │ ╰─────────' The type 'MyFirstPackage::my_module::Sword' does not have the ability 'drop'
   · │
34 │           assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
   │                                                                   ^ Invalid return
```

Bu hata mesajı oldukça karmaşık görünüyor, ancak neyin yanlış gittiğini anlamak için gereken tüm bilgileri içeriyor. Burada olan şey, testi yazarken yanlışlıkla Move dilinin güvenlik özelliklerinden birine rastlamış olmamızdır.

Unutmayın ki, `Sword` struct'ı, gerçek dünyadaki bir öğeyi dijital olarak taklit eden bir oyun varlığını temsil eder. Aynı zamanda, gerçek dünyadaki bir kılıç basitçe ortadan kaybolamazken (açıkça yok edilebilse de), dijital bir kılıç için böyle bir kısıtlama yoktur. Aslında, test fonksiyonumuzda olan tam olarak budur - fonksiyon çağrısının sonunda basitçe kaybolan bir `Sword` struct'ının bir örneğini oluşturuyoruz. Gördüğümüz hata mesajının özü de budur.

Çözümlerden biri (mesajın kendisinde önerildiği gibi), `Sword` struct'ının tanımına, bu struct'ın örneklerinin kaybolmasına (düşürülmesine) izin verecek `drop` yeteneğini eklemektir. Tartışmalı bir şekilde, değerli bir varlığı düşürebilmek, sahip olmak istediğimiz bir varlık özelliği değildir, bu nedenle sorunumuzun bir başka çözümü de kılıcın mülkiyetini devretmektir.

Testimizin çalışmasını sağlamak için, [Transfer modülünü](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/transfer.move) içe aktarmak üzere test fonksiyonumuzun başına aşağıdaki satırı ekliyoruz:

```
        use sui::transfer;

```

Daha sonra Test fonksiyonumuzun sonuna aşağıdaki satırları ekleyerek kılıcın sahipliğini yeni oluşturulmuş sahte bir adrese aktarmak için `Transfer` modülünü kullanırız:

```
        // create a dummy address and transfer the sword
        let dummy_address = @0xCAFE;
        transfer::transfer(sword, dummy_address);
```

Şimdi test komutunu tekrar çalıştırabilir ve gerçekten de tek bir başarılı testin çalıştırıldığını görebiliriz:

```
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyFirstPackage
Running Move unit tests
[ PASS    ] 0x0::my_module::test_sword_create
Test result: OK. Total tests: 1; passed: 1; failed: 0
```

***

İpucu: Birim testlerinin yalnızca bir alt kümesini çalıştırmak istiyorsanız, `--filter` seçeneğini kullanarak test adına göre filtreleme yapabilirsiniz. Örnek:

```
$ sui move test --filter sword
```

Yukarıdaki komut, adı "sword" içeren tüm testleri çalıştıracaktır. Daha fazla test seçeneğini keşfedebilirsiniz:

```
$ sui move test -h
```

***

#### Sui'ye özgü testler <a href="#sui-specific-testing" id="sui-specific-testing"></a>

Şimdiye kadar gördüğümüz test örneği büyük ölçüde saf Move'dur ve `sui::tx_context` ve `sui::transfer` gibi bazı Sui paketlerini kullanmanın ötesinde Sui ile çok az ilgisi vardır. Bu test tarzı, Sui için Move kodu yazan geliştiriciler için zaten çok yararlı olsa da, Sui'ye özgü ek özellikleri de test etmek isteyebilirler. Özellikle, Sui'deki bir Move çağrısı bir Sui [işlemi](https://docs.sui.io/devnet/learn/transactions) içinde kapsüllenir ve bir geliştirici tek bir test içinde farklı işlemler arasındaki etkileşimleri test etmek isteyebilir (örneğin, bir işlem bir nesne oluşturur ve diğeri onu aktarır).

Sui'ye özgü testler, saf Move ve [test framework'ünde](https://github.com/move-language/move/blob/main/language/documentation/book/src/unit-testing.md) bulunmayan Sui ile ilgili test işlevselliği sağlayan [test\_scenario modülü](https://github.com/MystenLabs/sui/blob/main/crates/sui-framework/sources/test\_scenario.move) aracılığıyla desteklenir.

`test_scenario`'daki ana kavram, her biri (potansiyel olarak) farklı bir kullanıcı tarafından yürütülen bir dizi Sui işlemini taklit eden bir senaryodur. Yüksek seviyede, test yazan bir geliştirici, ilk ve tek argüman olarak bu işlemi yürüten kullanıcının adresini alan ve bir senaryoyu temsil eden `Scenario` struct'ının bir örneğini döndüren `test_scenario::begin` fonksiyonunu kullanarak ilk işlemi başlatır.

`Scenario` yapısının bir örneği, Sui'nin nesne depolamasını taklit eden adres başına bir nesne havuzu içerir ve havuzdaki nesneleri işlemek için yardımcı işlevler sağlanır. İlk işlem tamamlandıktan sonra, mevcut senaryoyu temsil eden `Scenario` struct örneğini ve (yeni) bir kullanıcının adresini argüman olarak alan `test_scenario::next_tx` fonksiyonu kullanılarak sonraki işlemler başlatılabilir.

Çalışan örneğimizi, bir Sui geliştiricisinin bakış açısından kılıç oluşturma ve transferini test etmek için `test_scenario`'yu kullanan çoklu işlem testi ile genişletelim. İlk olarak, kılıç oluşturma ve transferini uygulayan Sui'den çağrılabilir giriş fonksiyonları oluşturalım ve bunları `my_module.move` dosyasına koyalım:

```
    public entry fun sword_create(magic: u64, strength: u64, recipient: address, ctx: &mut TxContext) {
        use sui::transfer;

        // create a sword
        let sword = Sword {
            id: object::new(ctx),
            magic: magic,
            strength: strength,
        };
        // transfer the sword
        transfer::transfer(sword, recipient);
    }

    public entry fun sword_transfer(sword: Sword, recipient: address, _ctx: &mut TxContext) {
        use sui::transfer;
        // transfer the sword
        transfer::transfer(sword, recipient);
    }
```

Yeni fonksiyonların kodu kendi kendini açıklayıcıdır ve önceki bölümlerde gördüğümüze benzer bir şekilde struct oluşturma ve Sui-iç modülleri (`TxContext` ve `Transfer`) kullanır. Önemli olan kısım, giriş fonksiyonlarının daha önce [açıklandığı ](https://docs.sui.io/devnet/build#entry-functions)gibi doğru imzalara sahip olmasıdır. Bu kodun oluşturulabilmesi için, `TxContext` yapısını fonksiyon tanımları için kullanılabilir hale getirmek üzere modül seviyesinde (modülün ana kod bloğunda mevcut modül genelindeki `ID` modülü içe aktarımından hemen önce ilk satır olarak) ek bir içe aktarım satırı eklememiz gerekir:

```
    use sui::tx_context::TxContext;
```

Artık yeni fonksiyonlarla genişletilmiş modülü oluşturabiliriz ancak hala sadece bir test tanımlanmış durumda. Başka bir test fonksiyonu ekleyerek bunu değiştirelim.

```
    #[test]
    fun test_sword_transactions() {
        use sui::test_scenario;

        // create test addresses representing users
        let admin = @0xBABE;
        let initial_owner = @0xCAFE;
        let final_owner = @0xFACE;

        // first transaction to emulate module initialization
        let scenario_val = test_scenario::begin(admin);
        let scenario = &mut scenario_val;
        {
            init(test_scenario::ctx(scenario));
        };
        // second transaction executed by admin to create the sword
        test_scenario::next_tx(scenario, admin);
        {
            let forge = test_scenario::take_from_sender<Forge>(scenario);
            // create the sword and transfer it to the initial owner
            sword_create(&mut forge, 42, 7, initial_owner, test_scenario::ctx(scenario));
            test_scenario::return_to_sender(scenario, forge)
        };
        // third transaction executed by the initial sword owner
        test_scenario::next_tx(scenario, initial_owner);
        {
            // extract the sword owned by the initial owner
            let sword = test_scenario::take_from_sender<Sword>(scenario);
            // transfer the sword to the final owner
            transfer::transfer(sword, final_owner);
        };
        // fourth transaction executed by the final sword owner
        test_scenario::next_tx(scenario, final_owner);
        {

            // extract the sword owned by the final owner
            let sword = test_scenario::take_from_sender<Sword>(scenario);
            // verify that the sword has expected properties
            assert!(magic(&sword) == 42 && strength(&sword) == 7, 1);
            // return the sword to the object pool (it cannot be simply "dropped")
            test_scenario::return_to_sender(scenario, sword)
        };
        test_scenario::end(scenario_val);
    }
```

Şimdi yeni test işlevinin bazı ayrıntılarına girelim. Yapacağımız ilk şey, test senaryosuna katılan kullanıcıları temsil eden bazı adresler oluşturmaktır. (Bir oyun yöneticisi kullanıcımız ve oyuncuları temsil eden iki normal kullanıcımız olduğunu varsayıyoruz). Daha sonra, yönetici adresi adına bir kılıç yaratan ve sahipliğini ilk sahibine aktaran ilk işlemi başlatarak bir senaryo oluşturuyoruz.

İkinci işlem ilk sahibi tarafından yürütülür (`test_scenario::next_tx` fonksiyonuna argüman olarak aktarılır) ve daha sonra artık sahibi olduğu kılıcı son sahibine aktarır. Lütfen _saf Move_'da Sui depolama kavramına sahip olmadığımızı ve sonuç olarak taklit edilmiş Sui işleminin onu depodan alması için kolay bir yol olmadığını unutmayın. İşte bu noktada `test_scenario` modülü yardıma gelir - `take_from_sender` fonksiyonu, mevcut işlemi yürüten bir adresin sahip olduğu belirli bir türdeki (bu durumda `Sword` türündeki) bir nesneyi Move kodu tarafından manipüle edilebilir hale getirir. (Şimdilik, böyle bir nesnenin yalnızca bir tane olduğunu varsayıyoruz.) Bu durumda, depodan alınan nesne başka bir adrese aktarılır.

> **Önemli:** Nesne oluşturma/aktarma gibi işlem etkileri yalnızca belirli bir işlem tamamlandıktan sonra görünür hale gelir. Örneğin, çalışan örneğimizdeki ikinci işlem bir kılıç oluşturup yöneticinin adresine aktardıysa, kılıç yalnızca üçüncü işlemde yöneticinin adresinden `(test_scenarios take_from_sender` veya `take_from_address` işlevleri aracılığıyla) alınabilir hale gelecektir.

Son işlem son sahip tarafından yürütülür - kılıç nesnesini depodan alır ve beklenen özelliklere sahip olup olmadığını kontrol eder. [Bir paketin test edilmesinde ](https://docs.sui.io/devnet/build/move/build-test#testing-a-package)açıklandığı gibi, saf Move test senaryosunda, bir nesne Move kodunda kullanılabilir olduğunda (örneğin, oluşturulduktan veya bu durumda öykünülmüş depodan alındıktan sonra), kolayca kaybolamayacağını unutmayın.

Saf Move test fonksiyonunda, kılıç nesnesini sahte adrese aktararak bu sorunu hallettik. Ancak `test_scenario` paketi bize, Move kodu Sui bağlamında gerçekten çalıştırıldığında ne olduğuna daha yakın olan daha zarif bir çözüm sunar - `test_scenario::return_to_sender` işlevini kullanarak kılıcı nesne havuzuna geri döndürebiliriz.

Şimdi test komutunu tekrar çalıştırabilir ve modülümüz için artık iki başarılı testimiz olduğunu görebiliriz:

```
BUILDING MoveStdlib
BUILDING Sui
BUILDING MyFirstPackage
Running Move unit tests
[ PASS    ] 0x0::my_module::test_sword_create
[ PASS    ] 0x0::my_module::test_sword_transactions
Test result: OK. Total tests: 2; passed: 2; failed: 0
```
