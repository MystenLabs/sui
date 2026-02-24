# Validatörler

Sui ağı, her biri Sui yazılımının kendi örneğini ayrı bir makinede (veya aynı varlık tarafından işletilen parçalanmış bir makine kümesinde) çalıştıran bir dizi bağımsız _validatör_ tarafından işletilmektedir. Bir validatör, istemciler tarafından gönderilen okuma ve yazma isteklerini ele alarak ağa katılır. Bu bölüm ikincisine odaklanmaktadır.

Sui, ağı hangi validatörlerin işlettiğini ve bunların oy gücünü belirlemek için hisse kanıtı (PoS) kullanır. Validatörler, işlem ücretlerinden bir pay, stake ödülleri ve yanlış davranışları cezalandırmak için kesinti yoluyla iyi niyetle katılmaya teşvik edilir.

### Epoch'lar <a href="#epochs" id="epochs"></a>

Sui ağının çalışması zamansal olarak çakışmayan, yaklaşık sabit süreli (örneğin 24 saatlik) _dönemlere (epochs)_ ayrılmıştır. Belirli bir dönem boyunca, ağa katılan validatör seti sabittir. Bir dönem sınırında, yeniden yapılandırma meydana gelebilir ve ağa katılan validatör kümesini ve bunların oylama gücünü değiştirebilir. Kavramsal olarak, yeniden yapılandırma Sui protokolünün yeni bir örneğini başlatır ve önceki dönemin son durumu [genesis ](https://docs.sui.io/devnet/build/cli-client#genesis)ve yeni validatörler kümesi operatörler olarak kabul edilir.

### Quorum'lar (Yeter Sayı) <a href="#quorums" id="quorums"></a>

_Yeter sayı_, belirli bir dönem boyunca birleşik oylama gücü toplamın >2/3'ü olan doğrulayıcılar kümesidir. Örneğin, hepsi aynı oy gücüne sahip dört validatör tarafından işletilen bir Sui örneğinde, üç validatör içeren herhangi bir grup bir yeter sayıdır.

Çekirdek büyüklüğü >2/3, [_Bizans hata_](https://en.wikipedia.org/wiki/Byzantine\_fault) _toleransını (BFT)_ sağlamak için seçilmiştir. Göreceğimiz gibi, bir validatör bir işlemi (yani, işlemi kalıcı olarak saklar ve dahili durumunu işlemin etkileriyle günceller) yalnızca bir çekirdekten kriptografik imzalar eşlik ediyorsa gerçekleştirir. İşlem ve baytlarındaki çekirdek imzalarının birleşimine sertifika diyoruz. Yalnızca sertifikaları taahhüt etme politikası Bizans hata toleransını garanti eder: validatörlerin >2/3'ü protokolü sadakatle takip ederse, sonunda hem taahhüt edilen sertifikalar kümesi hem de bunların etkileri üzerinde anlaşacakları garanti edilir.

### İstek yazın <a href="#write-requests" id="write-requests"></a>

Bir validator iki tür yazma isteğini işleyebilir: işlemler ve sertifikalar. Yüksek seviyede, bir istemci:

* Bir sertifika oluşturmak için gereken imzaları toplamak üzere bir validatör yeter sayısına bir işlem iletir.
* bir sertifikayı bir validatöre göndererek o validatördeki durum değişikliklerini işler.

#### İşlemler <a href="#transactions" id="transactions"></a>

Validator bir istemciden bir işlem aldığında, ilk olarak işlem geçerlilik kontrollerini gerçekleştirir (örneğin, gönderenin imzasının geçerliliği). Kontroller başarılı olursa, validatör tüm sahip olunan nesneleri kilitler ve işlem baytlarını imzalar. Daha sonra imzayı istemciye geri gönderir. İstemci, bir komiteden işlemiyle ilgili imzaları toplayana kadar bu işlemi birden fazla validatör ile tekrarlar ve böylece bir sertifika oluşturur.

Bir işlemdeki validatör imzalarını bir sertifikada toplama işleminin ve sertifikaları gönderme işleminin paralel olarak gerçekleştirilebileceğini unutmayın. İstemci, işlemleri/sertifikaları aynı anda rastgele sayıda validatöre çoklu gönderebilir. Alternatif olarak, bir istemci bu görevlerden birini veya her ikisini bir üçüncü taraf hizmet sağlayıcısına yaptırabilir. Bu sağlayıcıya canlılık açısından güvenilmelidir (örneğin, bir sertifika oluşturmayı reddedebilir), ancak güvenlik açısından güvenilmemelidir (örneğin, işlemin etkilerini değiştiremez ve kullanıcının gizli anahtarına ihtiyaç duymaz).

#### Sertifikalar <a href="#certificates" id="certificates"></a>

İstemci bir sertifika oluşturduktan sonra, sertifikayı sertifika geçerlilik kontrollerini gerçekleştirecek olan bir validatöre gönderir (örneğin, imzalayanların geçerli dönemdeki validatörler olduğundan ve imzaların kriptografik olarak geçerli olduğundan emin olmak gibi). Kontroller başarılı olursa, yetkili sertifika içindeki işlemi yürütür. Bir işlemin yürütülmesi ya başarılı olur ve tüm etkilerini deftere işler ya da iptal olur (örneğin, açık bir `abort`(iptal) talimatı, sıfıra bölme gibi bir çalışma zamanı hatası veya maksimum gas bütçesinin aşılması nedeniyle) ve işlemin gaz girdisini borçlandırmaktan başka hiçbir etkisi olmaz. Her iki durumda da işlem, iç işleminin hash'i tarafından indekslenen sertifikayı kalıcı olarak saklayacaktır.

İşlemlerde olduğu gibi, bir sertifikayı validatörlerle paylaşma sürecinin paralelleştirilebileceğini ve (istenirse) üçüncü taraf bir hizmet sağlayıcıya yaptırılabileceğini belirtmek isteriz. Bir istemci, (BFT varsayımlarına kadar) en az bir dürüst validatörün sertifikayı yürüttüğünden ve taahhüt ettiğinden emin olmak için sertifikasını validatörlerin >1/3'üne yayınlamalıdır. Diğer validatörler sertifikayı validatörler arası durum senkronizasyonu veya istemci destekli durum senkronizasyonu yoluyla öğrenebilir.

### Narwhal ve Bullshark'ın rolü <a href="#the-role-of-narwhal-and-bullshark" id="the-role-of-narwhal-and-bullshark"></a>

Sui, [Narwhal ve Tusk'tan yararlanır: DAG tabanlı bir Mempool ve Verimli BFT Konsensüsü](https://docs.sui.io/devnet/learn/architecture/consensus) ve Tusk halefi [Bullshark](https://arxiv.org/abs/2201.05677). Narwhal/Bullshark (N/B) da [Mysten Labs](https://mystenlabs.com/) tarafından uygulanmaktadır, böylece Bizans anlaşması gerektiğinde, farklı paylaşılan nesneler üzerinde yürütme paralelleştirilirken paylaşılan kilitleri yönetmek için yüksek verimli DAG tabanlı bir konsensüs kullanırız.

Narwhal, işlemlerin eşzamanlı olarak önerilen bloklarda toplanan gruplar halinde paralel olarak sıralanmasını sağlar ve Bullshark bu blokların oluşturduğu DAG'yi yürütmek için bir algoritma tanımlar. N/B, eşzamanlı olarak önerilen bloklardan oluşan bir DAG oluşturur ve DAG'ın oluşturulmasının bir yan ürünü olarak bu bloklar arasında bir düzen oluşturur. Ancak bu düzen Sui işlemlerinin nedensel düzeninin (Narwhal/Bullshark'ın buradaki "yükü") üzerine yerleştirilir ve onun yerine geçmez:

* Narwhal/Bullshark XO modu yerine OX modunda çalışır (O = sipariş, X = yürütme); yürütme Narwhal/Bullshark siparişinden sonra gerçekleşir.
* Bu nedenle N/B'nin çıktısı, işlem verilerinin kendisinde depolanan karşılıklı bağımlılıklarla birlikte bir dizi işlemdir.

İşlemlerin konsensüs dizileri sertifikaları. Bunlar, sahip oldukları tüm nesnelerin üzerinde işlem yapılabilecek durumda olduğunu kontrol eden ve işlemi imzalayan validatörlerin 2/3'üne sunulmuş olan işlemleri temsil eder. Bir sertifika sıralandıktan sonra, yaptığımız şey, paylaşılan nesnelerin kilidini, bu sertifikanın yürütülmesiyle eşleştirmek için bir sonraki mevcut sürümde ayarlamaktır. Örneğin, 2. sürümde paylaşılan bir X nesnemiz varsa ve T sertifikasını sıralarsak, T -> \[(X, 2)] olarak saklarız. Konsensüse ulaştığımızda yaptığımız tek şey budur ve sonuç olarak çok sayıda sıralı işlemi alabiliriz.

Şimdi, bu yapıldıktan sonra Sui, kilitleri ayarlanmış olan tüm sertifikaları bir veya birden fazla çekirdek üzerinde çalıştırabilir (şu anda). Açıkçası, nesnelerin önceki sürümleri için işlemlerin önce (nedensel olarak) işlenmesi gerekir ve bu da eşzamanlılık derecesini azaltır. İşlemin okuma ve yazma kümesi, sürümlendirilmiş nesne girdilerinden statik olarak belirlenebilir - yürütme yalnızca işleme girdi olan veya işlem tarafından oluşturulan bir nesneyi okuyabilir/yazabilir.

### İleri okumalar <a href="#further-reading" id="further-reading"></a>

* İşlemler girdi olarak nesneleri alır ve çıktı olarak nesneler üretir; nesnelerin yapısı ve nitelikleri hakkında daha fazla bilgi edinmek için nesneler bölümüne göz atın.
* Sui birkaç farklı işlem türünü destekler; tüm ayrıntılar için işlemler bölümüne bakın.
