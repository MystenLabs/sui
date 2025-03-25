---
description: Sui İşlemleri
---

# İşlemler

Sui ledger'daki tüm güncellemeler bir işlem aracılığıyla gerçekleşir. Bu konu Sui tarafından desteklenen işlem türlerini ve bunların yürütülmesinin ledger'ı nasıl değiştirdiğini açıklamaktadır.

Tüm Sui işlemleri aşağıdaki ortak meta verilere sahiptir:

* Gönderen adresi: Bu işlemi gönderen kullanıcının adresi.&#x20;
* Gas Input'u: Bu işlemin yürütülmesi ve depolanması için ödeme yapmak üzere kullanılacak nesneye işaret eden bir nesne referansı. Bu nesne kullanıcıya ait olmalı ve `sui::coin::Coin<SUI`(yani Sui yerel para birimi) türünde olmalıdır.
* Gas Fiyatı: Bu işlemin ödeyeceği gas birimi başına yerel token sayısını belirten işaretsiz bir tamsayı. Gas fiyatı her zaman sıfırdan farklı olmalıdır.&#x20;
* Maksimum Gas Bütçesi: Bu işlemin yürütülmesiyle harcanabilecek maksimum gas birimi sayısı. Bu bütçe aşılırsa, işlem yürütme işlemi iptal edilir ve gas girdisinin borçlandırılması dışında hiçbir etkisi olmaz. Gas input nesnesi, gas fiyatının maksimum gas ile çarpımından daha yüksek bir değere sahip olmalıdır ve bu çarpım, gas input nesnesinin işlem için borçlandırılacağı maksimum tutardır.&#x20;
* Epoch: Bu işlemin amaçlandığı Sui dönemi.&#x20;
* Tür: Bir çağrı, yayınlama veya yerel işlem ve onun türe özgü verileri (aşağıya bakın).&#x20;
* Kimlik Doğrulayıcı: Yukarıdaki verilerin Binary Canonical Serialization (BCS) ile kodlanmış baytları üzerinde kriptografik bir imza ve hem imzaya karşı doğrulayan hem de gönderen adresi tarafından kriptografik olarak taahhüt edilen bir açık anahtar.

### Move çağrısı işlemi <a href="#move-call-transaction" id="move-call-transaction"></a>

Bu işlem türü, göndericinin sahip olduğu nesneler ve girdi olarak saf değerlerle (ör. tamsayılar) yayınlanmış bir Move paketindeki bir işlevi çağıran bir _akıllı kontrat_ çağrısıdır. Bir işlevin yürütülmesi, bu girdi nesnelerinin yanı sıra yürütme sırasında oluşturulan diğer nesneleri de okuyabilir, yazabilir, değiştirebilir ve aktarabilir.

Yukarıdaki ortak meta verilere ek olarak, bir çağrı işlemi aşağıdaki alanları içerir:

* Paket: Daha önce yayınlanmış bir Move paketi nesnesine işaret eden bir nesne referansı.&#x20;
* Modül: Paket içindeki Move modülünün adını belirten UTF-8 dizesi. İşlev: Modül içindeki bir işlevin adını belirten bir UTF-8 dizesi. İşlev geçerli bir giriş noktası olmalıdır.&#x20;
* Tip Girişleri: Fonksiyonun tip parametrelerine bağlanacak Move tiplerinin bir listesi.&#x20;
* Nesne Girişleri: Bu fonksiyona aktarılacak nesnelere işaret eden benzersiz nesne referanslarının bir listesi. Her nesne ya göndericiye ait olmalı, ya değişmez olmalı ya da paylaşılmalıdır. Yukarıdaki gas girdi nesnesi aynı zamanda bir nesne girdisi olarak görünemez.&#x20;
* Saf Girdiler: Fonksiyonun parametrelerine bağlanacak BCS kodlu değerlerin bir listesi. Saf girdiler ilkel tipler (yani adresler, nesne kimlikleri, dizeler, baytlar, tamsayılar veya booleanlar) olmalıdır - nesne olamazlar.

### Move işlemi yayınlama <a href="#move-publish-transaction" id="move-publish-transaction"></a>

Bu işlem türü, yeni bir Move paketini değişmez bir nesne olarak yayınlar. Paket yayınlandıktan sonra, genel işlevleri ve türleri gelecekteki paketler tarafından kullanılabilir ve giriş noktası işlevleri gelecekteki işlemler tarafından çağrılabilir.

Yukarıdaki ortak meta verilere ek olarak, bir yayınlama işlemi Paket Baytları içerir: Bağımlılık ilişkilerine göre topolojik olarak sıralanmış Move bytecode modüllerinin bir listesi (yani, bağımlılık grafiğindeki yapraklar listede daha önce görünmelidir). Bu modüller serileştirilecek, doğrulanacak ve bağımlılıklarıyla ilişkilendirilecektir. Ayrıca, her modülün başlatıcı işlevi listede belirtilen sırada çağrılacaktır.

### Yerel işlem <a href="#native-transaction" id="native-transaction"></a>

Yerel işlemler, yaygın Sui işlemlerinin optimize edilmiş versiyonlarıdır. Her bir yerel işlem anlamsal olarak belirli bir Move çağrısına eşdeğerdir ancak daha düşük gas maliyetine sahiptir.

#### Nesneyi transfer et <a href="#transfer-object" id="transfer-object"></a>

Bu işlem türü, nesneleri göndericiden belirtilen alıcılara aktarır.

Yukarıdaki ortak meta verilere ek olarak, bir aktarım nesnesi işlemi aşağıdaki alanları içerir:

* Girdi: Gönderenin sahip olduğu değiştirilebilir bir nesneye işaret eden bir nesne referansı. Nesne, genel aktarımlara izin veren türde olmalıdır - yani, depolama yeteneğine sahip herhangi bir tür. Yukarıdaki gas giriş nesnesi aynı zamanda nesne girişi olarak görünemez.
* Alıcı: Bu aktarımdan nesneyi alacak olan adres.

#### SUI transfer et <a href="#transfer-sui" id="transfer-sui"></a>

This transaction type is similar to the Transfer Object transaction type, but the input object type must be a SUI coin--that is, an object of type `sui::coin::Coin<sui::sui::SUI>`. The benefit of this transaction type is that a separate coin object is not needed for gas. The gas payment is taken from the SUI coin being transferred. Optionally, an amount can be specified for partial transfers.

Bu işlem türü Transfer Nesnesi işlem türüne benzer, ancak girdi nesne türü bir SUI coin olmalıdır - yani `sui::coin::Coin<sui::sui::SUI>`türünde bir nesne. Bu işlem türünün avantajı, gas için ayrı bir coin nesnesine ihtiyaç duyulmamasıdır. Gas ödemesi transfer edilen SUI coin'den alınır. İsteğe bağlı olarak, kısmi transferler için bir miktar belirtilebilir.

Yukarıdaki ortak meta verilere ek olarak, bir transfer SUI işlemi aşağıdaki alanları içerir:

* Input: An object reference pointing to a `sui::coin::Coin<sui::sui::SUI>` object owned by the sender.
* Girdi: Gönderenin sahip olduğu bir `sui::coin::Coin<sui::sui::SUI>` nesnesine işaret eden bir nesne referansı.
* (İsteğe bağlı) Tutar: Alıcının alacağı tutarı kodlayan işaretsiz bir tamsayı. Tutar, giriş nesnesinden çekilir, yeni oluşturulmuş bir jeton nesnesine sarılır ve ilgili alıcı adresine gönderilir. Girdi nesnesinin değeri, belirtilen tutardan büyük veya eşit olmalıdır.
* Alıcı: Bu transferden madeni parayı alacak olan adres.

#### Katıl (Join) <a href="#join" id="join"></a>

Bu işlem türü birkaç madeni para nesnesini bir araya getirir. Aşağıdaki alanı içerir:

Girdiler: Göndericinin sahip olduğu değiştirilebilir nesnelere işaret eden benzersiz nesne referanslarının bir listesi. Nesnelerin hepsi aynı tipte olmalıdır: `sui::coin::Coin<T>` ile keyfi `T`--yani, herhangi bir değiştirilebilir belirteç. Liste en az iki nesne içermelidir. İlki hariç tüm nesneler yok edilecek ve ilk nesnenin yeni değeri eski değeri artı yok edilen tüm nesnelerin değerlerinin toplamı olacaktır. Yukarıdaki gas giriş nesnesi aynı zamanda bir nesne girişi olarak görünemez.

### İleri Okumalar <a href="#further-reading" id="further-reading"></a>

* Sui akıllı kontratlarını geliştirmek için Move eğitimine bakın.
* İşlemler nesneleri girdi olarak alır ve çıktı olarak nesneler üretir - nesneler, yapıları ve nitelikleri hakkında bilgi edinin.
* İşlemler Sui validatörleri tarafından yürütülür.
