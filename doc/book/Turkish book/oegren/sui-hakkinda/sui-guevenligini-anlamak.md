# Sui Güvenliğini Anlamak

Bu sayfa, Sui'nin Güvenlik açısından sağladığı başlıca garantilere genel bir bakış sunmaktadır.

Sui varlık sahipleri ve akıllı kontrat tasarımcıları, varlıklarını güvence altına almak için mevcut mekanizmalar ve Sui'nin onlar için sağladığı güvenceler hakkında bilgi edinmeye buradan başlayabilirler. Akıllı kontrat tasarımcıları da tasarladıkları varlık türlerinin varlık sahiplerine güvenli bir deneyim sunmak üzere Sui'den yararlanmasını sağlamak için genel Sui güvenlik mimarisi hakkında bilgi edinebilirler.

Bu konuya girmeden önce Sui'nin temel bileşenlerine aşina olmak için okumayı unutmayın: [Sui Nasıl Çalışır?](https://docs.sui.io/devnet/learn/how-sui-works)

### Güvenlik Özellikleri <a href="#security-features" id="security-features"></a>

Sui'yi varlık sahiplerine çok yüksek güvenlik garantileri sağlayacak şekilde tasarladık. Sui'deki varlıkların, denetlenebilen akıllı kontratlarla önceden tanımlanmış mantığa göre yalnızca sahipleri tarafından kullanılabilmesini ve Sui'yi işleten bazı validatörlerin protokolü doğru şekilde takip etmemesine rağmen ağın bunları doğru şekilde işlemeye hazır olmasını sağlıyoruz (hata toleransı).

Sui sisteminin güvenlik özellikleri bir dizi özelliği garanti eder:

* Yalnızca sahip olunan bir varlığın sahibi, bu varlık üzerinde çalışan bir işlemi yetkilendirebilir. Yetkilendirme, yalnızca varlık sahibi tarafından bilinen özel bir imza anahtarı kullanılarak gerçekleştirilir.
* Herkes paylaşılan varlıklar ya da değişmez varlıklar üzerinde işlem yapabilir ancak akıllı kontrat tarafından ek erişim kontrol mantığı uygulanabilir.-
* İşlemler, varlık türünü tanımlayan akıllı kontrat yaratıcısı tarafından belirlenen önceden tanımlanmış kurallara göre varlıklar üzerinde çalışır. Bunlar Move dili kullanılarak ifade edilir.
* Bir işlem sonuçlandırıldığında, etkileri - yani üzerinde çalıştığı varlıklardaki değişiklikler veya oluşturulan yeni varlıklar - kalıcı hale getirilecek ve ortaya çıkan varlıklar daha fazla işlem için kullanılabilir olacaktır.
* Sui sistemi, bir dizi bağımsız validatör arasında bir protokol aracılığıyla çalışır. Yine de validatörlerin küçük bir kısmı protokole uymadığında tüm güvenlik özellikleri korunur.
* Sui'deki tüm işlemler, herhangi bir varlığın doğru şekilde işlendiğinden emin olmak için denetlenebilir. Bu, Sui üzerindeki tüm işlemlerin herkes tarafından görülebileceği anlamına gelir ve kullanıcılar gizliliklerini korumak için birden fazla farklı adres kullanmak isteyebilir.
* Validatörler, Sui kullanıcılarının SUI belirteçlerini kilitlemesi ve bir veya daha fazla validatöre delege etmesi yoluyla periyodik olarak belirlenir.

### Güvenlik mimarisi <a href="#security-architecture" id="security-architecture"></a>

Sui sistemi, işlemleri işleyen bir dizi validatör tarafından işletilmektedir. Sistemde sunulan ve işlenen geçerli işlemler üzerinde anlaşmaya varmalarını sağlayan Sui protokolünü uygularlar.

Sui'nin kullandığı anlaşma protokolleri, Bizans hata toleransı yayını ve consensus kullanımı yoluyla Sui protokolünü doğru şekilde takip etmeyen validatörlerin bir kısmını tolere eder. Özellikle, her validatör, kullanıcıların SUI belirteçlerini kullanarak kendileri için yetki verme / oylama süreci yoluyla kendisine atanan bir miktar oylama gücüne sahiptir. Payların 2/3'ünden fazlası protokolü takip eden validatörlere atanırsa Sui tüm güvenlik özelliklerini korur. Bununla birlikte, daha fazla validatör hatalı olsa bile bir dizi denetim özelliği korunur.

#### Adresler ve mülkiyet <a href="#addresses-and-ownership" id="addresses-and-ownership"></a>

Bir Sui işlemi, ancak üzerinde çalıştığı tüm varlıkların sahibi özel imza anahtarıyla (şu anda EdDSA algoritması kullanılıyor) işlemi dijital olarak imzalarsa geçerli olur ve devam edebilir. Bu imza anahtarı kullanıcı tarafından gizli tutulabilir ve başka kimseyle paylaşılamaz. Sonuç olarak, tüm validatörler protokolü takip etmese bile, başka herhangi bir tarafın bir kullanıcının sahip olduğu bir varlık üzerinde fark edilmeden işlem yapması mümkün değildir.

Özel bir imza anahtarı aynı zamanda Sui ağında bir kullanıcı varlıkları göndermek veya akıllı kontratları özel erişim kontrol mantığı tanımlamasına izin vermek için kullanılabilecek bir genel adrese karşılık gelir. Bir kullanıcı, kolaylık veya gizlilik nedenleriyle birden fazla imza anahtarına karşılık gelen bir veya daha fazla adrese sahip olabilir. Bir adresin herhangi bir ön kayda ihtiyacı yoktur ve bir adrese varlık göndermek bu adresi ağ üzerinde otomatik olarak oluşturur. Ancak bu, yanlış bir adrese varlık göndermenin geri alınamaz etkileri olabileceğinden, kullanıcıların transferlerin alıcı adresini veya diğer işlemlere dahil olan tarafları kontrol etmeye dikkat etmeleri gerektiği anlamına gelir.

#### Akıllı kontratlar varlık türlerini ve varlık türlerinin mantığını tanımlar <a href="#smart-contracts-define-asset-types-and-their-logic" id="smart-contracts-define-asset-types-and-their-logic"></a>

Tüm varlıkların bir Sui Akıllı Kontratı içinde tanımlanan bir türü vardır. Sui, SUI yerel token'ını yönetmek için kullanılanlar gibi birkaç sistem kontratı sağlar, ancak herkesin özel akıllı kontratı yazmasına ve göndermesine de izin verir. Bir varlık türü üzerindeki bir işlem, yalnızca varlık türünü tanımlayan akıllı kontratlarda tanımlanan işlemleri çağırabilir ve kontrattaki mantık tarafından kısıtlanır.

Bu nedenle, kullanıcıların güvendikleri, kendilerinin veya güvendikleri diğer kişilerin denetlediği akıllı kontratları kullanarak varlıkları üzerinde işlem yapmaları ve varlıkları üzerindeki işlemler için tanımladıkları mantığı anlamaları teşvik edilmektedir. Sui akıllı kontratları, üçüncü tarafların bunları denetlemesine izin vermek ve ayrıca güvenceyi artırmak için değiştirilmelerini önlemek için değişmez varlıklar olarak tanımlanır. Sui'nin kullandığı Move akıllı kontrat dili, denetim ve doğrulama kolaylığı göz önünde bulundurularak tasarlanmıştır. [Move'da Akıllı Kontratlara giriş](https://docs.sui.io/devnet/build/move) yazımız ilginizi çekebilir.

Paylaşılan varlıklar, birden fazla kullanıcının işlemler aracılığıyla bunlar üzerinde işlem yapmasına olanak tanır; bu, sahip olunan varlıkların bazılarının yanı sıra bir veya daha fazla paylaşılan varlığı da içerebilir. Bu paylaşılan varlıklar, paylaşılan varlığın türünü tanımlayan akıllı kontrata göre, farklı kullanıcılar arasında güvenli bir şekilde aracılık eden protokolleri uygulamak için kullanılan veri ve mantığı temsil eder. Sui, tüm kullanıcıların paylaşılan varlıkları içeren işlemler oluşturmasına izin verir. Ancak akıllı kontrat türü, paylaşılan varlıkların hangi adreste ve nasıl kullanılabileceğine ilişkin ek kısıtlamalar tanımlayabilir.

#### İşlem kesinliği <a href="#transaction-finality" id="transaction-finality"></a>

Tüm validatörlere sunulan geçerli bir işlemin sertifikalandırılması ve sertifikasının da sonuçlandırılması için tüm validatörlere sunulması gerekir. Validatörlerin bir alt kümesi protokolü takip etmese bile, işlem Sui protokolünü doğru bir şekilde takip eden kalan validatörler aracılığıyla sonuçlandırılabilir. Bu, Sui protokolü tarafından tanımlanan yayın ve consensus için kriptografik Bizans hata toleransı anlaşma protokollerinin kullanılmasıyla elde edilir. Bu protokoller hem güvenliği, yani hatalı validatörlerin doğru istemcileri hatalı durum konusunda ikna edememesini hem de canlılığı, yani hatalı validatörlerin işlemin yapılmasını engelleyememesini sağlar.

Sui'deki tüm işlemler, Sui tarafından işleme maliyetini karşılamak için bir gas varlığı ile ilişkilendirilmelidir. Geçerli bir işlem, başarılı bir yürütme veya iptal edilmiş bir yürütme durumuyla sonuçlanabilir. Yürütme, varlığı tanımlayan akıllı kontrattaki bir koşul nedeniyle veya yürütme maliyetini ödemek için yeterli gas'in tükenmesi nedeniyle iptal edilebilir. Başarı durumunda, işlemin etkileri sonlandırılır; aksi takdirde, işlemdeki varlıkların durumu değişmez. Ancak, bir bütün olarak sisteme yönelik hizmet reddi saldırılarını hafifletmek için gas varlığından her zaman bir miktar gas tahsil edilir.

Bir kullanıcı istemcisi işlemi ve sertifikayı gönderme işlemini kendisi gerçekleştirebilir veya işlemi göndermek ve validatörlerle etkileşime geçmek için üçüncü taraf hizmetlerine güvenebilir. Bu tür üçüncü tarafların kullanıcı özel imza anahtarlarına sahip olması gerekmez ve kullanıcılar adına işlem sahteciliği yapamazlar. Kullanıcı müşteriye, işlemin kesinliğini ve etkilerini onaylayan validatörlerden gelen bir dizi imza aracılığıyla bir işlemin sonuçlandırıldığına dair güvence verebilirler. Bu noktadan sonra kullanıcılar işlemin yol açtığı değişikliklerin kesinleştiğinden emin olabilirler.

#### Denetim ve gizlilik <a href="#auditing-and-privacy" id="auditing-and-privacy"></a>

Sui validatörleri, kullanıcıların depoladıkları tüm varlıkları ve bu varlıklara yol açan işlemlerin geçmiş kayıtlarını okumaları için olanaklar sağlar. Validatörler ayrıca bir varlık durumuna katkıda bulunan tüm işlem zincirinin kriptografik kanıtını da sağlar. Kullanıcı istemcileri, tüm işlemlerin doğru olduğundan ve validatörler arasındaki ortak anlaşmanın sonucu olduğundan emin olmak için bu kanıt zincirini talep edebilir ve doğrulayabilir. Bir veya daha fazla validatörün durumunu yansıtan eksiksiz replikaları işleten hizmetler bu tür denetimleri rutin olarak gerçekleştirir.

Sui'nin aşırı kamusal denetlenebilirliği, Sui'deki tüm işlemlerin ve varlıkların kamuya açık olduğu anlamına da gelir. Gizliliklerine dikkat eden kullanıcılar, bir dereceye kadar takma addan ya da üçüncü taraf saklama veya saklama dışı hizmetlerden faydalanmak için birden fazla adres kullanabilir. Ek kriptografik gizlilik korumalarına sahip özel akıllı kontratlar da üçüncü taraflarca sağlanabilir.

#### Sansüre direnç ve açıklık <a href="#censorship-resistance-and-openness" id="censorship-resistance-and-openness"></a>

Sui, validatörleri periyodik olarak belirlemek için yerleşik "Delegated Proof-of Stake" modelini kullanır. Kullanıcılar, bir sonraki dönemde Sui ağını işleten validatörleri belirlemek için her dönemde SUI tokenlerini kilitleyebilir ve delege edebilirler. Asgari bir miktarın üzerinde delege edilmiş hisseye sahip olan herkes bir validatör çalıştırabilir.

Validatörler ağı işletir ve Sui'lerini validatör olarak desteklemeleri için delege eden kullanıcılara gas ücreti geliri yoluyla ödüller sağlar. Güvenilirliği düşük olan validatörler ve dolayısıyla hisselerini onlara devreden kullanıcılar daha düşük bir ödül alabilirler. Ancak kullanıcı hisselerine kötü niyetli validatörler ya da ağdaki herhangi biri tarafından el konulamaz.

Bu mekanizma, validatörlerin Sui kullanıcılarına karşı sorumlu olmasını ve geçerli işlemleri sansürlemeye yönelik fark edilen girişimler de dahil olmak üzere ilk güvenilmezlik veya yanlış davranış belirtisinde rotasyona tabi tutulabilmesini sağlar. Sui kullanıcıları, validatörleri ve işletmek istedikleri protokolü seçerek Sui sisteminin gelecekteki gelişimi üzerinde de anlamlı bir söz hakkına sahip olurlar.

### İleri Okumalar <a href="#further-reading" id="further-reading"></a>

Sui güvenliğinin arkasındaki bilgisayar biliminin derinlemesine teknik bir açıklamasını arıyorsanız, [Sui Akıllı Kontratlar Platformu](https://docs.sui.io/paper/sui.pdf) hakkındaki white paper'ımıza göz atabilirsiniz.
