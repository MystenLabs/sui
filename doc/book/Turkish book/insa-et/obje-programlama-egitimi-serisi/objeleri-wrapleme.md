# Objeleri Wrap'leme

Birçok programlama dilinde, karmaşık veri yapılarını başka bir veri yapısının içine yerleştirerek veri yapılarını katmanlar halinde düzenleriz. Move'da, aşağıdaki gibi `struct` türünde bir alanı başka bir alanın içine koyarak aynı şeyi yapabilirsiniz:

```
struct Foo has key {
    id: UID,
    bar: Bar,
}

struct Bar has store {
    value: u64,
}
```

> 📖Bir struct türünün bir Sui nesne struct'ına (anahtar yeteneğine sahip olacak) gömülebilmesi için, gömülen struct türünün `store` yeteneğine sahip olması gerekir.

Yukarıdaki örnekte, `Bar` normal bir Move yapısıdır, ancak `key` özelliğine sahip olmadığı için bir Sui nesnesi değildir. Bu, sadece iyi bir kapsülleme ile veri düzenlememiz gerektiğinde yaygın bir kullanımdır. Ancak bazı durumlarda, bir Sui nesnesi struct türünü başka bir Sui nesnesi struct türüne alan olarak koymak isteriz. Yukarıdaki örnekte, `Bar`'ı şu şekilde değiştirebiliriz:

```
struct Bar has key, store {
    id: UID,
    value: u64,
}
```

Şimdi `Bar` da bir Sui nesne tipidir. `Bar` türünden bir Sui nesnesini `Foo` türünden bir Sui nesnesinin içine koyduğumuzda, `Bar` türünden Sui nesnesinin `Foo` tarafından **wrap**'lendiği söylenir (buna **wrapping** nesne veya **wrapper** nesne diyoruz).

> 💡_Move kodunda, bir Sui nesnesini Sui olmayan bir struct türünün alanı olarak koymak da mümkündür. Örneğin, yukarıdaki kod örneğinde, `Foo`'yu `key`'i olmayacak ancak `Bar`'ı `key`'i olacak şekilde tanımlayabiliriz, `store` edebiliriz. Ancak, bu durum yalnızca bir Move yürütmesinin ortasında geçici olarak gerçekleşebilir ve zincir üzerinde kalıcı olamaz. Bunun nedeni, Sui olmayan bir nesnenin Move-Sui sınırı boyunca akamaması ve bir noktada Sui olmayan nesnenin paketinden çıkarılması ve içindeki Sui nesne alanlarıyla ilgilenilmesi gerektiğidir._

Bir Sui nesnesini başka bir nesneye sarmalamanın bazı ilginç sonuçları vardır. Bir nesne sarıldığında, bu nesne artık zincir üzerinde bağımsız olarak var olmaz. Artık bu nesneyi kimliğine göre arayamayacağız. Bu nesne, onu saran nesnenin verilerinin bir parçası haline gelir. _En önemlisi, artık Move çağrılarında sarmalanmış nesneyi herhangi bir şekilde argüman olarak iletemeyiz._ Tek erişim noktası sarmalayan nesnedir.

> 💡Sarılmış bir Sui nesnesini artık kullanamayacağınız gerçeği, A'nın B'yi, B'nin C'yi ve C'nin de A'yı sardığı döngüsel sarma davranışı oluşturmanın imkansız olduğu anlamına gelir.

Bir noktada, sarılmış nesneyi çıkarabilir ve bir adrese aktarabilirsiniz. Bu işleme **unwrapping** denir. Bir nesne **unwrapped** edildiğinde, tekrar bağımsız bir nesne haline gelir ve doğrudan zincir üzerinde erişilebilir. Wrapping ve unwrapping ile ilgili önemli bir özellik de vardır: nesnenin ID'si wrapping ve unwrapping boyunca aynı kalır!

Bir Sui nesnesini başka bir Sui nesnesine wrap etmenin birkaç yaygın yolu vardır ve bunların kullanım durumları genellikle farklıdır. Aşağıda, bir Sui nesnesini wrap etmenin üç farklı yolunu ve bunların tipik kullanım durumlarını inceleyeceğiz.

#### Direct wrapping <a href="#direct-wrapping" id="direct-wrapping"></a>

Bir Sui nesne türünü doğrudan başka bir Sui nesne türüne alan olarak koyduğumuzda (tıpkı `Bar`'ı `Foo`'ya bar alanı olarak koyduğumuz gibi), buna _doğrudan wrapping_ denir. Doğrudan wrapping ile elde edilen en önemli özellik şudur: _Wrapped nesnesini yok etmediğimiz sürece wrapped nesnesi unwrapped edilemez._ Yukarıdaki örnekte, `bar`'ı tekrar bağımsız bir nesne haline getirmek için `Foo` nesnesini silmek (ve dolayısıyla [unpack](https://move-book.com/advanced-topics/struct.html#destructing-structures) etmek) Bu özellik nedeniyle, doğrudan wrapping, nesne kilitlemeyi uygulamanın en iyi yoludur: bir nesneyi kısıtlı erişimle kilitleyin ve yalnızca belirli sözleşme çağrıları yoluyla kilidini açabilirsiniz.

Doğrudan wrapping'in nasıl kullanılacağını göstermek için örnek bir güvenilir takas uygulaması üzerinden gidelim. Diyelim ki `scarcity` ve `style` olan NFT tarzı bir `Nesne` türü var. `scarcity`, nesnenin ne kadar nadir olduğunu belirler (muhtemelen ne kadar nadir olursa piyasa değeri o kadar yüksek olur); `style` ise nesnenin içeriğini/türünü veya nasıl işlendiğini belirler. Diyelim ki bu nesnelerden bazılarına sahipsiniz ve nesnelerinizi başkalarıyla takas etmek istiyorsunuz. Ancak bunun adil bir takas olduğundan emin olmak için, bir nesneyi yalnızca aynı `scarcity`'e sahip ancak farklı bir `style`'a sahip başka bir nesneyle takas etmek istiyorsunuz (böylece daha fazla stil toplayabilirsiniz).

Öncelikle böyle bir nesne türü tanımlayalım:

```
struct Object has key, store {
    id: UID,
    scarcity: u8,
    style: u8,
}
```

Gerçek bir uygulamada, muhtemelen nesnelerin sınırlı bir arzı olduğundan ve bunları bir sahipler listesine basmak için bir mekanizma olduğundan emin oluruz. Basitlik ve gösterim amacıyla, burada sadece oluşturulmasını kolaylaştıracağız:

```
public entry fun create_object(scarcity: u8, style: u8, ctx: &mut TxContext) {
    let object = Object {
        id: object::new(ctx),
        scarcity,
        style,
    };
    transfer::transfer(object, tx_context::sender(ctx))
}
```

Herkes belirtilen kıtlık ve stile sahip yeni bir nesne oluşturmak için `create_object` çağrısı yapabilir. Oluşturulan nesne, işlemi imzalayan kişiye gönderilecektir. Muhtemelen nesneyi başkalarına da aktarabilmek isteyeceğiz:

```
public entry fun transfer_object(object: Object, recipient: address) {
    transfer::transfer(object, recipient)
}
```

Şimdi kendi nesneniz ile başkalarının nesneleri arasında bir takası/alışverişi nasıl sağlayabileceğimize bakalım. Basit fikir şudur: iki adresten iki nesne alan ve sahipliklerini takas eden bir fonksiyon tanımlayın. Ancak bu Sui'de çalışmaz! [Bölüm 2](https://docs.sui.io/devnet/build/programming-with-objects/ch2-using-objects)'den sadece nesne sahiplerinin nesneyi değiştirmek için bir işlem gönderebileceğini hatırlayın. Yani bir kişi kendi nesnesini başkasının nesnesi ile değiştirecek bir işlem gönderemez.

Gelecekte, bu tür kullanım durumlarında iki kişinin aynı işlemi imzalayabilmesi için muhtemelen çoklu imzalı işlemler sunacağız. Ancak, her zaman hemen takas edecek birini bulamayabilirsiniz. Çoklu imzalı bir işlem bu senaryoda işe yaramayacaktır. Bulabilseniz bile, bir takas hedefi bulma yükünü taşımak istemeyebilirsiniz.

Diğer bir yaygın çözüm ise nesnenizi bir havuza (örneğin NFT durumunda bir pazaryeri ya da tokenlar durumunda bir likidite havuzu) "göndermek" ve takası havuzda (hemen ya da daha sonra talep olduğunda) gerçekleştirmektir. Gelecek bölümlerde, herkes tarafından mutasyona uğratılabilen paylaşılan nesneler kavramını inceleyecek ve bunun herkesin paylaşılan bir nesne havuzunda çalışmasına nasıl olanak sağladığını göstereceğiz. Bu bölümde, sahip olunan nesneleri kullanarak aynı etkiyi nasıl elde edeceğimize odaklanacağız. Sadece sahip olunan nesneleri kullanan işlemler, Sui'de mutabakat gerektirmediğinden, paylaşılan nesneleri kullanmaktan daha hızlı ve daha ucuzdur (gaz açısından).

Nesnelerin takas edilebilmesi için her iki nesnenin de aynı adrese ait olması gerekir. Üçüncü bir tarafın takas hizmetleri sağlamak için altyapı kurduğunu düşünebiliriz. Nesnelerini takas etmek isteyen herkes nesnelerini üçüncü tarafa gönderebilir ve üçüncü taraf takasın gerçekleştirilmesine ve nesnelerin geri gönderilmesine yardımcı olur. Ancak üçüncü tarafa tam olarak güvenmiyoruz ve nesnelerimizin tam velayetini onlara vermek istemiyoruz. Bunu başarmak için doğrudan sarmalamayı kullanabiliriz. Aşağıdaki gibi bir sarmalayıcı nesne türü tanımlarız:

```
struct ObjectWrapper has key {
    id: UID,
    original_owner: address,
    to_swap: Object,
    fee: Balance<SUI>,
}
```

`ObjectWrapper` bir Sui nesne tipi tanımlar, takas etmek istediğimiz nesneyi `to_swap` olarak sarar ve nesnenin orijinal sahibini `original_owner`'da izler. Bunu daha ilginç ve gerçekçi kılmak için, bu takas için üçüncü tarafa bir miktar ücret ödememiz gerekebileceğini de bekleyebiliriz. Aşağıda, bir `Object`'e sahip olan biri tarafından takas talebinde bulunmak için bir arayüz tanımlıyoruz:

```
public entry fun request_swap(object: Object, fee: Coin<SUI>, service_address: address, ctx: &mut TxContext) {
    assert!(coin::value(&fee) >= MIN_FEE, 0);
    let wrapper = ObjectWrapper {
        id: object::new(ctx),
        original_owner: tx_context::sender(ctx),
        to_swap: object,
        fee: coin::into_balance(fee),
    };
    transfer::transfer(wrapper, service_address);
}
```

Yukarıdaki giriş fonksiyonunda, bir `Object`'in takas edilmesini talep etmek için, nesnenin tamamen tüketilmesi ve `ObjectWrapper`'a sarılması için değer olarak iletilmesi gerekir. Bir ücret (`Coin<SUI` türünde) de sağlanır. Fonksiyon ayrıca ücretin yeterli olup olmadığını da kontrol eder. `Wrapper` nesnesine koyarken `Coin`'i `Balance`'a dönüştürdüğümüze dikkat edin. Bunun nedeni, `Coin`'in bir Sui nesnesi türü olması ve yalnızca Sui nesneleri olarak (örneğin giriş işlevi argümanları veya adreslere gönderilen nesneler olarak) dolaşmak için kullanılmasıdır. Başka bir Sui nesne yapısına gömülmesi gereken madeni para bakiyeleri için bunun yerine `Balance` kullanırız çünkü bu bir Sui nesne türü değildir ve dolayısıyla kullanımı çok daha ucuzdur. Wrapper nesnesi daha sonra adresi çağrıda `service_address` olarak belirtilen servis operatörüne gönderilir.

Servis operatörü (`service_address`) artık takas edilecek nesneyi içeren `ObjectWrapper`'a sahip olsa da, servis operatörü hala temel wrapped `Object`'e erişemez veya çalamaz. Bunun nedeni, tanımladığımız `transfer_object` fonksiyonunun, çağıranın içine bir `Object` geçirmesini gerektirmesidir; ancak servis operatörü wrapped `Object`'e erişemez ve `ObjectWrapper`'ı `transfer_object` fonksiyonuna geçirmek geçersiz olur. Bir nesnenin yalnızca tanımlandığı modül tarafından okunabileceğini veya değiştirilebileceğini hatırlayın; bu modül yalnızca bir wrapping / paketleme işlevi (`request_swap`) tanımladığından ve bir unwrapping / unpacking işlevi tanımlamadığından, servis operatörünün wrapped `Object`'i almak için `ObjectWrapper`'ı açmasının bir yolu yoktur. Ayrıca, `ObjectWrapper`'ın kendisinde tanımlanmış herhangi bir aktarım yöntemi yoktur, bu nedenle hizmet operatörü sarılmış nesneyi başka birine de aktaramaz.

Son olarak, iki adresten gönderilen iki nesne arasında bir takas gerçekleştirmek için servis operatörünün çağırabileceği fonksiyonu tanımlayalım. Fonksiyon arayüzü şuna benzeyecektir:

```
public entry fun execute_swap(wrapper1: ObjectWrapper, wrapper2: ObjectWrapper, ctx: &mut TxContext);
```

Burada `wrapper1` ve `wrapper2`, farklı nesne sahiplerinden hizmet operatörüne gönderilen iki wrapped nesnedir. (Dolayısıyla, hizmet operatörü her ikisine de sahiptir.) Her iki wrapped nesne de değer olarak aktarılır çünkü eninde sonunda [unpack](https://move-book.com/advanced-topics/struct.html#destructing-structures) edilmeleri gerekecektir. İlk olarak takasın gerçekten yasal olup olmadığını kontrol ederiz:

```
assert!(wrapper1.to_swap.scarcity == wrapper2.to_swap.scarcity, 0);
assert!(wrapper1.to_swap.style != wrapper2.to_swap.style, 0);
```

İki nesnenin aynı kıtlığa sahip olduğunu, ancak farklı stile sahip olduğunu, bir takas için mükemmel bir çift olduğunu kontrol eder. Daha sonra iç alanları elde etmek için iki nesnenin paketini açıyoruz. Bunu yaparak nesnelerin paketini açmış oluruz:

```
let ObjectWrapper {
    id: id1,
    original_owner: original_owner1,
    to_swap: object1,
    fee: fee1,
} = wrapper1;

let ObjectWrapper {
    id: id2,
    original_owner: original_owner2,
    to_swap: object2,
    fee: fee2,
} = wrapper2;
```

Artık gerçek takas için ihtiyacımız olan her şeye sahibiz:

```
transfer::transfer(object1, original_owner2);
transfer::transfer(object2, original_owner1);
```

Yukarıdaki kod takas işlemini gerçekleştirir: `nesne1`'i `nesne2`'nin asıl sahibine gönderir ve `nesne1`'i `nesne2`'nin asıl sahibine gönderir. Hizmet sağlayıcı da ücreti almaktan mutluluk duyar:

```
let service_address = tx_context::sender(ctx);
balance::join(&mut fee1, fee2);
transfer::transfer(coin::from_balance(fee1, ctx), service_address);
```

`fee2`, `fee1` ile birleştirilir, bir `Coin`'e dönüştürülür ve `service_address` adresine gönderilir. Son olarak, Sui'ye her iki sarmalayıcı nesneyi de sildiğimizi bildiririz:

```
object::delete(id1);
object::delete(id2);
```

Bu çağrının sonunda, iki nesne takas edilmiş (karşı sahibine gönderilmiş) ve hizmet sağlayıcı hizmet ücretini almıştır.

Sözleşme `ObjectWrapper` ile başa çıkmanın tek bir yolunu tanımladığından - `execute_swap` - hizmet operatörünün sahipliğine rağmen `ObjectWrapper` ile etkileşime girebileceği başka bir yol yoktur.

Kaynak kodun tamamı [trusted\_swap.move](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/objects\_tutorial/sources/trusted\_swap.move) dosyasında bulunabilir.

Doğrudan wrapping kullanımının daha karmaşık bir örneği [escrow.move](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/defi/sources/escrow.move) dosyasında bulunabilir.

`Option` **aracılığıyla Wrapping**

Sui nesne türü `Bar` doğrudan `Foo`'ya wrapped edildiğinde, çok fazla esneklik olmaz: bir `Foo` nesnesi içinde bir `Bar` nesnesine sahip olmalıdır ve `Bar` nesnesini çıkarmak için `Foo` nesnesini yok etmek gerekir. Ancak, daha fazla esneklik istediğimiz durumlar vardır: sarma türü her zaman içinde wrapped nesneye sahip olabilir veya olmayabilir ve wrapped nesne bir noktada farklı bir nesne ile değiştirilebilir.

Bu kullanım durumunu basit bir oyun karakteri tasarlayarak gösterelim: Kılıcı ve kalkanı olan bir savaşçı. Bir savaşçının kılıcı ve kalkanı olabilir ya da olmayabilir ve istediği zaman bunları değiştirebilmelidir. Bunu tasarlamak için aşağıdaki gibi bir `SimpleWarrior` tipi tanımlıyoruz:

```
struct SimpleWarrior has key {
    id: UID,
    sword: Option<Sword>,
    shield: Option<Shield>,
}
```

Her `SimpleWarrior` tipi, şu şekilde tanımlanan isteğe bağlı bir kılıç ve kalkana sahiptir:

```
struct Sword has key, store {
    id: UID,
    strength: u8,
}

struct Shield has key, store {
    id: UID,
    armor: u8,
}
```

Yeni bir savaşçı yaratırken, henüz ekipman olmadığını belirtmek için `sword` ve `shield`'ı yok olarak ayarlayabiliriz:

```
public entry fun create_warrior(ctx: &mut TxContext) {
    let warrior = SimpleWarrior {
        id: object::new(ctx),
        sword: option::none(),
        shield: option::none(),
    };
    transfer::transfer(warrior, tx_context::sender(ctx))
}
```

Bununla, yeni kılıçlar veya yeni kalkanlar donatmak için fonksiyonlar tanımlayabiliriz:

```
public entry fun equip_sword(warrior: &mut SimpleWarrior, sword: Sword, ctx: &mut TxContext) {
    if (option::is_some(&warrior.sword)) {
        let old_sword = option::extract(&mut warrior.sword);
        transfer::transfer(old_sword, tx_context::sender(ctx));
    };
    option::fill(&mut warrior.sword, sword);
}
```

Yukarıdaki fonksiyonda, bir **warrior** `SimpleWarrior`'un değişebilir referansı olarak ve bir `sword`**'u** değer olarak geçiriyoruz çünkü onu `warrior`'a wrap etmemiz gerekiyor.

`Sword`, bırakma yeteneği olmayan bir Sui nesne türü olduğundan, savaşçının zaten bir kılıcı varsa, bu kılıcın öylece `drop`'lanamayacağına dikkat etmek önemlidir. Önce mevcut kılıcı kontrol edip çıkarmadan `option::fill`'e bir çağrı yaparsak, çalışma zamanı hatası oluşabilir. Bu nedenle, `equip_sword`'da, önce zaten bir kılıç olup olmadığını kontrol ederiz ve eğer varsa, onu çıkarır ve gönderene geri göndeririz. Bu, yeni bir kılıç kuşandığınızda beklediğiniz şeyle eşleşir - eğer varsa eski kılıcı geri alırsınız.

Kodun tamamını [simple\_warrior.move](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/objects\_tutorial/sources/simple\_warrior.move) dosyasında bulabilirsiniz.

Ayrıca [hero.move](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/games/sources/hero.move)'da daha karmaşık bir örnek bulabilirsiniz.

`vector` **üzerinden wrapping**

Nesneleri başka bir Sui nesnesinin vektör alanına wrapping kavramı `Option` üzerinden wrapping'e çok benzer: bir nesne 0, 1 veya aynı türden birçok wrapped nesne içerebilir.

Bu kullanım durumunu göstermek için tam bir örnek kullanmayacağız, ancak vektör aracılığıyla wrapping benzeyebilir:

```
struct Pet has key, store {
    id: UID,
    cuteness: u64,
}

struct Farm has key {
    id: UID,
    pets: vector<Pet>,
}
```

Yukarıdaki örnekte, bir `Pet` vektörü `Farm`'a sarılmıştır ve yalnızca `Farm` nesnesi aracılığıyla erişilebilir.
