# Sui Gezgini

[Sui Explorer](https://explorer.sui.io/), Sui ağındaki işlemler ve faaliyetler hakkındaki verileri görüntülemenizi sağlar. Ağdaki etkinliği görüntülemenin yanı sıra, Gezgini şu amaçlarla da kullanabilirsiniz:

* Sui ağındaki etkinlik ve metrikler hakkında güncel bilgileri görüntüleyin.
* Varlıklarınızı ve sözleşmelerinizi arayın, doğrulayın ve takip edin.
* Sorunları belirlemeye ve çözmeye yardımcı olması için hızlı, güvenilir, denetim verilerinden ve şeffaf hata ayıklamadan yararlanın.
* Sui'de paketler olarak adlandırılan tüm akıllı sözleşmeler için tanımlama desteği alın.
* Şu anda aktif olan tam düğümlerin doğrulayıcılarını ve coğrafi konumlarını görüntüleyin.

Sui Gezgini'nin yerel olarak nasıl çalıştırılacağına ilişkin talimatlar için [Sui Gezgini beni oku ](https://github.com/MystenLabs/sui/tree/main/apps/explorer#readme)bölümüne bakın.

### Bir ağ seçin <a href="#choose-a-network" id="choose-a-network"></a>

Sui Gezgini'ni başlattığınızda, varsayılan olarak Devnet ağı için olan işlemleri görüntüler. Gezgin'i ortamınızda çalışan yerel bir ağın veya özel bir RPC uç noktası URL'sinin verilerini görüntülemek için de kullanabilirsiniz. Farklı bir ağ seçmek için sayfanın sağ üst köşesindeki açılır menüyü kullanın.

### İşleminizi bulun <a href="#finding-your-transaction" id="finding-your-transaction"></a>

Bir adres, nesne kimliği veya işlem kimliği kullanarak işlemleri arayabilirsiniz. Örneğin, bir işlemi onaylamak veya onayladığınız bir işlemle ilgili ek ayrıntıları görüntülemek için cüzdan adresinizi arayabilirsiniz. Nasıl cüzdan oluşturulacağını öğrenmek için [Sui Cüzdanı](https://docs.sui.io/devnet/explore/wallet-browser) bölümüne bakın.

**Sui Cüzdanı kullanılarak yapılan bir işlemi aramak için**

1. Sui Cüzdanınızı açın.
2. Cüzdan adresinizi kopyalamak için pano simgesine tıklayın.
3. Sui Gezgini'ni açın.
4. O anda seçili değilse **Coins** sekmesini seçin.
5. Arama alanına cüzdan adresinizi yapıştırın ve ardından **Enter** tuşuna basın.

Explorer, cüzdan adresiniz için **Adres** ayrıntıları sayfasını görüntüler. Explorer'da cüzdan geçmişinizde gördüğünüz işlemin aynısını görmelisiniz. Ayrıntılarını görüntülemek için bir işlemin üzerine tıklayın.

### Gezgin başlangıç sayfası <a href="#explorer-start-page" id="explorer-start-page"></a>

Sui Gezgin'ini açtığınızda, sayfa ağdaki işlemleri ve en iyi validatörleri görüntüler. **İşlemler** tablosunda ilk olarak en son işlem listelenir.

**En İyi Validatörler** tablosu ağdaki en iyi doğrulayıcıları listeler ve ağdaki tüm node'ların coğrafi konumlarını gösteren bir harita içerir.

**İşlemler** sayfasını açmak ve ağdaki tüm işlemleri görüntülemek için **Daha Fazla İşlem'e** tıklayın.

### İşlemler <a href="#transactions" id="transactions"></a>

**İşlemler** sayfası ağdaki tüm işlemleri listeler. Sayfa başına 20, 40 veya 60 işlem satırı görüntüleyebilirsiniz. Sayfa başına görüntülenen satır sayısını değiştirmek için sayfanın sağ alt köşesindeki açılır menüyü kullanın. Daha fazla işlem görüntülemek için sayfanın altındaki sayfa seçici simgelerini kullanın.

#### İşlem tablosu sütunları <a href="#transaction-table-columns" id="transaction-table-columns"></a>

Sayfadaki **İşlemler** tablosu, aşağıdaki sütunları içerir:

* **Zaman** - İşlemin gerçekleştiği zaman.
* **Tür** - İşlem türü, Call, TransferSui, TransferObject veya Publish seçeneklerinden biridir.
  * **Çağrı** - Bir hareket modülünü çağrıştıran bir işlem. Çağrı işlemleri için tablo yalnızca gönderen adresini içerir.
  * **TransferSui** - Sui'yi bir adresten diğerine aktarma işlemi.
  * **TransferObject** - Bir nesneyi farklı bir adrese aktarma işlemi.
  * **Yayınlama** - Bir paketi yayınlamak için yapılan işlem.
  * **Toplu** - Toplu işlemler
* **İşlem Kimliği (ID)** - İşlemin benzersiz tanımlayıcısı. Kimliği kopyalamak için pano simgesine tıklayın. İşlemle ilgili ayrıntıları görüntülemek için **İşlem Kimliğ**i sütunundaki bir değere tıklayın.
* **Adresler** - İşlem ile ilgili gönderici ve alıcıların adresleri. Ek ayrıntılar ve adres kullanılarak yapılan işlemler için bir adrese tıklayabilirsiniz.
* **Miktar** - İşlem için kullanılan coin sayısı ve coin türü.
* **Gas (işlem ücreti)** - İşlemi tamamlamak için gereken gas'ı ödemek için kullanılan Sui miktarı.

İşlemin veya adresin ayrıntılar sayfasını açmak için **İşlem Kimliği** veya **Adresler** sütunundaki bir değere tıklayabilirsiniz. Bir işlem kimliğine tıkladığınızda açılan sayfa işlemin türüne bağlıdır. Sui Gezgini aşağıdaki ayrıntı sayfalarını sağlar:

* Her işlem türü için [işlem ayrıntıları](https://docs.sui.io/devnet/explore/sui-explorer#transaction-details-pages)
  * TransferSui
  * TransferObject
  * Çağrı
  * Yayınlama
  * Toplu işlemler
* [Nesne ayrıntıları](https://docs.sui.io/devnet/explore/sui-explorer#object-details-page)
* [Adres detayları](https://docs.sui.io/devnet/explore/sui-explorer#address-details-page)
* [Paket detayları](https://docs.sui.io/devnet/explore/sui-explorer#package-details-page)

### İşlem detayları <a href="#transaction-details" id="transaction-details"></a>

Bir **İşlem Kimliğine** tıkladığınızda bir ayrıntılar sayfası açılır. Sayfa başlığı işlem türünü yansıtır ve görüntülenen alanlar işlem türüne bağlı olarak değişir. Alanlardan birini görmüyorsanız, bunun nedeni seçilen işlem türü için mevcut olmamasıdır. Örneğin, bir TransferSui işlemi Etkinlikler sekmesi içermez.

İşlem ayrıntıları sayfaları aşağıdaki sekmeleri içerir:

* **Detaylar** - Provides additional details about the transaction.
* **Olaylar** - İşlemle ilişkili olayları görüntüler.
* **İmzalar** - İşlem için validatörlerden gelen imzaları listeler.

**Ayrıntılar** sekmesi aşağıdaki alanları içerir:

* **Paket Detayları** - Yalnızca Çağrı işlemleri için görüntülenir.
  * **Paket Kimliği (ID)** - Çağrı işlemiyle ilişkili paketin kimliği.
  * **Modül** - İşlem için kullanılan modül.
  * **İşlev** - İşlem için adı geçen işlev.
  * **Argüman** - İşlevle birlikte verilen tüm argümanlar
* **Güncel** - İşlemin güncellediği nesnenin nesne kimliği.
* **Oluşturma** - Bu işlemin oluşturduğu nesnenin nesne kimliği.
* **Miktar** - İşlem için aktarılan coin'lerin sayısı ve türü.
* **Gönderen** - İşlemin göndericisinin adresi. Yalnızca yayınlama işlemleri için görüntülenir.
* **Gönderici & Alıcılar** -İşlemle ilişkili adresler. İlk değer gönderenin adresidir ve yeşil onay işaretinin yanındaki adres alıcının adresidir. Birden fazla alıcı olduğunda, alan birden fazla adres içerir.
* **Modules** - İşlemi oluşturmak ve yürütmek için kullanılan modül kodunu gösterir.
* **Gas ve depolama ücretleri** - İşlem için gas ve ücretler hakkında ayrıntıları içerir

Gas Ödemesi için geçerli değer, işlem için kullanılan coin nesnesinin nesne kimliğidir.

* **Gas Ücretleri** - İşlem için kullanılan gas birimi sayısı.
* **Gas Bütçesi** - İşlem için izin verilen maksimum gas birimi sayısı.

**Olaylar** sekmesi, işlemin oluşturduğu olayları ve her olayla ilgili ayrıntıları listeler. TransferSui işlemleri olayları içermez.

**İmzalar** sekmesi aşağıdaki alanları içerir:

* **İşlem İmzaları** - İşlem için atılan imza veya imzalar.
* **Validatör İmzaları** - İşlemi doğrulayan validatörlerden gelen imzalar.

### Nesne detayları <a href="#object-details" id="object-details"></a>

Bir işlem ayrıntıları sayfasında görüntülenen bir nesne kimliğine tıkladığınızda, nesne ile ilişkili işlemler gibi nesnenin ayrıntılarını görüntüleyen bir sayfa açılır.

Sayfa aşağıdaki detayları içermektedir:

* **Açıklama**
  * **Tür** - Nesnenin türü, örneğin coin
  * **Nesne Kimliği (ID)** - Nesnenin kimliği.
  * **Son İşlem Kimliği (ID)** - Nesneyle ilişkili en son işlemin kimliği.
  * **Sürüm** - Nesnenin sürümü.
  * **Sahip** - Nesnenin sahibinin adresi.
* **Özellikler** - Nesne için olan detaylar, örneğin coin bakiyesi
* **Alt Nesneler** - Bu nesnenin sahip olduğu nesneler.

**İşlemler** - İşlemler sayfası ile aynı bilgilere sahiptir, ancak nesne ile ilişkili işlemlerle sınırlıdır.

### Addres detayları <a href="#address-details" id="address-details"></a>

**Adres** ayrıntıları sayfası, adresin sahip olduğu varlıklar ve adresle etkileşime giren işlemler dahil olmak üzere belirli bir adres hakkındaki ayrıntıları görüntülemenizi sağlar.

**Adres** ayrıntıları sayfası aşağıdaki alanları içerir:

* **Sahip olunan nesneler** - Adresin sahip olduğu nesneleri gösterir. Örneğin coinler
* **Coinler** - Sahip olunan coinlerin listesi ve coin türüne göre toplam bakiyeleri. Tek tek coin nesneleri hakkında ek ayrıntıları görüntülemek için bir girişe tıklayın.
* **NFTler** - Adresin sahip olduğu NFT'lerin listesi. NFT'nin nesne ayrıntıları sayfasını görüntülemek için bir kimliğe tıklayın.
* **İşlemler** - Her bir işlem hakkında daha ayrıntılı bilgi görüntülemek için tıklayın.

### Paket detayları <a href="#package-details" id="package-details"></a>

**Paket** ayrıntıları sayfasında paketin nesne kimliği, sürümü ve yayıncısı görüntülenir. Ayrıca, her birinin kodu da dahil olmak üzere kullanılan modülleri gösterir.

**Paket** ayrıntıları sayfası aşağıdaki alanları içerir:

* **Detaylar**
  * **Nesne Kimliği (ID)** - The object ID of the package.
  * **Sürüm** - Paketin sürümü.
  * **Yayıncı** - Paketin yayıncısı.
* **Modüller** - Pakette kullanılan modülleri listeler.
