# Nesneler

Sui'deki temel depolama birimi **nesnedir** (**object**). Depolamanın hesaplar etrafında toplandığı ve her hesabın bir anahtar-değer deposu içerdiği diğer birçok blockchain'in aksine, Sui'nin depolaması nesneler etrafında toplanmıştır. Bir akıllı kontrat bir nesnedir (**Move Package** olarak adlandırılır) ve bu akıllı kontratlar **Move nesnelerini** (**Move objects**)manipüle eder:

* _Move Paketi (Move Package)_: bir dizi Move bytecode modülü. Her modülün paket içinde benzersiz bir adı vardır. Paket kimliği ve bir modülün adının birleşimi, modülü benzersiz bir şekilde tanımlar. Akıllı kontratları Sui'de yayınladığımızda, bir paket yayınlama birimidir. Bir paket nesnesi yayınlandıktan sonra değişmezdir ve asla değiştirilemez veya kaldırılamaz. Bir paket nesnesi, daha önce Sui defterinde yayınlanmış olan diğer paket nesnelerine bağlı olabilir.
* _Move Nesnesi (Move Object)_: Bir Move paketinden belirli bir Move [_modülü_ ](https://github.com/move-language/move/blob/main/language/documentation/book/src/modules-and-scripts.md)tarafından yönetilen tiplendirilmiş veriler. Her nesne değeri, ilkel türleri (örneğin tamsayılar, adresler), diğer nesneleri ve nesne olmayan yapıları içerebilen alanları olan bir [yapıdır](https://github.com/move-language/move/blob/main/language/documentation/book/src/structs-and-resources.md). Her nesne değeri değiştirilebilir ve oluşturulduğu anda bir adrese aittir, ancak daha sonra _dondurulabilir_ ve kalıcı olarak değişmez hale gelebilir veya _paylaşılabilir_ ve böylece diğer adresler tarafından erişilebilir hale gelebilir.

Tüm Sui nesneleri aşağıdaki meta verilere sahiptir:

* 20 baytlık küresel olarak benzersiz bir kimlik. Nesne kimliği, nesneyi oluşturan işlemin özetinden ve işlem tarafından oluşturulan kimlik sayısını kodlayan bir sayaçtan türetilir.
* Bu nesneyi bir çıktı olarak içeren işlemlerin sayısını temsil eden 8 baytlık işaretsiz bir tamsayı _sürümü_. Böylece, bir işlem tarafından yeni oluşturulan tüm nesneler 1 sürümüne sahip olacaktır.
* Bu nesneyi bir çıktı olarak içeren son işlemi gösteren 32 baytlık bir _işlem özeti_.
* Bu nesneye nasıl erişilebileceğini gösteren 21 baytlık bir _sahip_ alanı. Nesne sahipliği bir sonraki bölümde ayrıntılı olarak açıklanacaktır.

Ortak meta verilere ek olarak, nesnelerin kategoriye özgü, değişken boyutlu bir içerik alanı vardır. Bir veri değeri için, bu alan nesnenin Move türünü ve [Binary Canonical Serialization (BCS)](https://docs.rs/bcs/latest/bcs/) kodlu yükünü içerir. Bir paket değeri için bu, paketteki bayt kodu modüllerini içerir.

### Nesne Sahipliği <a href="#object-ownership" id="object-ownership"></a>

Her nesnenin, bu nesneye nasıl sahip olunduğunu gösteren bir _sahiplik_ alanı vardır. Sahiplik, bir nesnenin işlemlerde nasıl kullanılabileceğini belirler. 4 farklı sahiplik türü vardır:

#### Sahibi bir adres <a href="#owned-by-an-address" id="owned-by-an-address"></a>

Bu, Move nesneleri için en yaygın durumdur. Move kodunda oluşturulduktan sonra bir Move nesnesi bir adrese [aktarılabilir](https://docs.sui.io/devnet/build/move/sui-move-library). Aktarımdan sonra, bu nesne o adrese ait olacaktır. Bir adres tarafından sahip olunan bir nesne, yalnızca o adres tarafından imzalanan işlemler tarafından kullanılabilir (yani Move çağrı parametresi olarak geçirilebilir). Sahip olunan nesne Move çağrı parametresi olarak 3 formdan herhangi birinde geçirilebilir: salt okunur referans (`&T`), değiştirilebilir referans (`&mut T`) ve by-value (`T`). Bir Move çağrısında bir nesne salt okunur referans (`&T`) ile aktarılsa bile, yine de yalnızca nesnenin sahibinin böyle bir çağrı yapabileceğine dikkat etmek önemlidir. Yani, bir nesnenin bir işlemde kullanılıp kullanılamayacağının doğrulanması söz konusu olduğunda Move çağrısının amacı önemsizdir, önemli olan sahipliktir.

#### Sahibi başka bir nesne <a href="#owned-by-another-object" id="owned-by-another-object"></a>

Bir nesne başka bir nesne tarafından sahiplenilebilir. Bu doğrudan sahipliği _nesne wrapleme_ işleminden ayırmak önemlidir. Bir nesnenin struct tanımının bir alanı başka bir nesne türü olduğunda, bir nesne başka bir nesneye wraplenebilir/gömülebilir. Örneğin:

```
struct A {
    id: UID,
    b: B,
}
```

Bu durumda, `B` türündeki bir nesnenin `A` türündeki bir nesneye sarıldığını söyleriz. Nesne sarma ile, sarılan nesne (bu örnekte,`b` nesnesi) Sui depolama alanında üst düzey bir nesne olarak saklanmaz ve nesne kimliği ile erişilemez. Bunun yerine, `A` türündeki bir nesnenin serileştirilmiş bayt içeriğinin bir parçasıdır. Bir nesnenin sarılması durumunu silinmesine benzer şekilde düşünebilirsiniz, ancak içeriği hala başka bir nesnede bir yerlerde mevcuttur. Şimdi başka bir nesne tarafından sahiplenilen nesne konusuna geri dönelim. Bir nesne başka bir nesne tarafından sahiplenildiğinde, sarılmaz. Bu, alt nesnenin hala üst düzey bir nesne olarak bağımsız bir şekilde var olduğu ve Sui depolama alanında doğrudan erişilebileceği anlamına gelir. Sahiplik ilişkisi yalnızca alt nesnenin sahip alanı aracılığıyla izlenir. Bu, alt nesneyi hala gözlemlemek veya diğer işlemlerde kullanabilmek istiyorsanız faydalı olabilir. Bir nesneyi başka bir nesnenin sahibi yapmak için kütüphane API'leri sağlıyoruz. Bunun nasıl yapılacağı hakkında daha fazla ayrıntı [Sui Move kütüphanesinde](https://docs.sui.io/devnet/build/move/sui-move-library) bulunabilir.

#### Değiştirilemez <a href="#immutable" id="immutable"></a>

Bu, bir nesnenin değişmez olduğu ve hiç kimse tarafından değiştirilemeyeceği anlamına gelir. Bu nedenle, böyle bir nesnenin özel bir sahibi yoktur. Herkes onu Move çağrılarında kullanabilir. Tüm Move paketleri değişmez nesnelerdir: bir kez yayınlandıktan sonra değiştirilemezler. Bir Move nesnesi [_freeze\_object_](https://docs.sui.io/devnet/build/move/sui-move-library) kütüphane API'si aracılığıyla değişmez bir nesneye dönüştürülebilir. Değişmez bir nesne, Move çağrılarında yalnızca salt okunur bir referans (`&T`) olarak aktarılabilir.

#### Paylaşılmış <a href="#shared" id="shared"></a>

Bir nesne paylaşılabilir, yani herkes bu nesneyi okuyabilir veya yazabilir. Sahip olunan değiştirilebilir nesnelerin (tek yazarlı) aksine, paylaşılan nesneler okuma ve yazma işlemlerini sıralamak için [konsensüs ](https://docs.sui.io/devnet/learn/architecture/consensus)gerektirir. Paylaşılan bir nesne oluşturma ve bu nesneye erişme örneği için [https://examples.sui.io/](https://examples.sui.io/) adresindeki [Paylaşılan Nesne](https://examples.sui.io/basics/shared-object.html#shared-object) bölümüne bakın.

Diğer blockchainlerde her nesne paylaşılır. Ancak, Sui programcıları genellikle belirli bir kullanım durumunu paylaşılan nesneler, sahip olunan nesneler veya bunların bir kombinasyonunu kullanarak uygulama seçeneğine sahiptir. Bu seçimin performans, güvenlik ve uygulama karmaşıklığı üzerinde etkileri olabilir. Bu ödünleşimleri anlamanın en iyi yolu, her iki şekilde de uygulanan birkaç kullanım durumu örneğine bakmaktır:

Emanet: [Paylaşılan](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/defi/sources/shared\_escrow.move), [Sahipli ](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/defi/sources/escrow.move)Açık Artırma: [Paylaşılan](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/nfts/sources/shared\_auction.move), Sahipli Tic Tac Toe: [Paylaşılan](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/games/sources/shared\_tic\_tac\_toe.move), [Sahipli](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/games/sources/tic\_tac\_toe.move)

### Nesnelere atıfta bulunma <a href="#referring-to-objects" id="referring-to-objects"></a>

Bir nesneye, tam içeriğini ve meta verilerini belirtmeden kısaca atıfta bulunmanın, her biri biraz farklı kullanım durumlarına sahip birkaç farklı yolu vardır:

* ID: yukarıda bahsedilen nesnenin global olarak benzersiz ID'si. Bu, zaman içinde nesne için sabit bir tanımlayıcıdır ve bir nesnenin mevcut durumunu sorgulamak veya iki adres arasında hangi nesnenin aktarıldığını tanımlamak için kullanışlıdır.
* Sürümlü Kimlik: bir (Kimlik, sürüm) çifti. Bu, nesnenin geçmişindeki belirli bir noktada nesnenin durumunu tanımlar ve geçmişte bir noktada nesnenin değerinin ne olduğunu sormak veya bir nesnenin bazı görünümlerinin şu anda ne kadar yeni olduğunu belirlemek için kullanışlıdır.
* Nesne Referansı: bir (ID, sürüm, nesne özeti) üçlüsü. Nesne özeti, nesnenin içeriğinin ve meta verilerinin karmasıdır. Bir nesne referansı, nesnenin geçmişindeki belirli bir noktada nesnenin doğrulanmış bir görünümünü sağlar. İşlemler, işlemin göndericisinin ve işlemi işleyen bir validatörün nesnenin içeriği ve meta verileri konusunda hemfikir olmasını sağlamak için nesne girdilerinin nesne referansları aracılığıyla belirtilmesini gerektirir.

### İşlem-nesne DAG'ı: Nesneleri ve işlemleri ilişkilendirme <a href="#the-transaction-object-dag-relating-objects-and-transactions" id="the-transaction-object-dag-relating-objects-and-transactions"></a>

İşlemler (ve dolayısıyla sertifikalar) nesneleri girdi olarak alır, bu girdileri okur/yazar/mutasyona uğratır ve çıktı olarak mutasyona uğramış ya da yeni oluşturulmuş nesneler üretir. Ve yukarıda tartışıldığı gibi, her nesne kendisini çıktı olarak üreten son işlemi (hash) bilir. Bu nedenle, nesneler ve işlemler arasındaki ilişkiyi temsil etmenin doğal bir yolu, yönlendirilmiş asiklik bir grafiktir (DAG):

* node'lar işlemlerdir.
* yönlendirilmiş kenarlar işlem çıktı nesnelerini işlem girdi nesnelerine bağlar ve nesne referansları ile etiketlenir.

Bu grafiği oluşturmak için, taahhüt edilen her işlem için bir düğüm ekleriz ve `A`'nın `O` nesnesini üretmesi (yani, `O`'yu oluşturması veya mutasyona uğratması) ve `B` işleminin `O` nesnesini girdi olarak alması durumunda `A` işleminden `B` işlemine `O` nesne referansı ile etiketlenmiş bir yönlendirilmiş kenar çizeriz.

Bu DAG'ın kökü, hiçbir girdi almayan ve sistemin _genesis_ durumunda var olan nesneleri üreten bir oluşum işlemidir. DAG, henüz herhangi bir işlem tarafından tüketilmemiş olan değiştirilebilir işlem çıktıları belirlenerek ve bu çıktıları (ve isteğe bağlı olarak değiştirilemez işlem çıktılarını) girdi olarak alan yeni bir işlem gönderilerek genişletilebilir.

Bir işlem tarafından girdi olarak alınabilecek nesneler kümesi canlı nesnelerdir ve Sui tarafından tutulan global durum bu tür nesnelerin toplamından oluşur. Belirli bir `A` Sui adresi için canlı nesneler, sistemdeki tüm değişmez nesnelerle birlikte `A`'nın sahip olduğu tüm nesnelerdir.

Bu DAG sistemdeki tüm işlenmiş işlemleri içerdiğinde, sistemin durumu ve geçmişinin eksiksiz (ve kriptografik olarak denetlenebilir) bir görünümünü oluşturur. Buna ek olarak, yukarıdaki şemayı kullanarak bir işlem veya nesne alt kümesi (örneğin, tek bir adresin sahip olduğu nesneler) için ilgili geçmişin bir DAG'ını oluşturabiliriz.
