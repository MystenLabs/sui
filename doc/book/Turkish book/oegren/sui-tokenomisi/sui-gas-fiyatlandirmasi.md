# Sui Gas Fiyatlandırması

Sui'nin gas fiyatlandırma mekanizması, kullanıcılara düşük, öngörülebilir işlem ücretleri sunma, doğrulayıcıları işlem işleme operasyonlarını optimize etmeye teşvik etme ve hizmet reddi saldırılarını önleme gibi üçlü sonuçlara ulaşmaktadır.

Bu, gas ücretlerinin mevcut piyasa fiyatını tahmin etmek zorunda kalma endişesi olmadan Sui ağını kullanmaya odaklanabilen Sui kullanıcılarına iyi bir kullanıcı deneyimi sunar. Validatörler her dönemin başında ağ çapında bir referans fiyat üzerinde anlaştıklarından, Sui kullanıcıları işlemlerini gönderirken referans fiyatı güvenilir bir çapa olarak kullanırlar. Dahası, fiyat belirleme mekanizması iyi validatör davranışını ödüllendirmek için tasarlanmıştır, böylece SUI token sahipleri, ağın operatörleri (yani validatörler) ve kullanıcıları arasındaki teşvikleri hizalar.

Sui'nin gas fiyat mekanizmasının benzersiz bir özelliği, kullanıcıların işlem yürütme ve her işlemle ilişkili verilerin depolanması için ayrı ücretler ödemesidir. Rastgele bir işlemle ilişkili gas ücretleri $\tau$ eşittir:

$GasFees\[\tau] \ = \ ComputationUnits\[\tau] \times ComputationPrice\[\tau] \ + \ StorageUnits\[\tau] \times StoragePrice$

Gas fonksiyonları $ComputationUnits\[\tau]$ ve $StorageUnits\[\tau]$ sırasıyla $\tau$ ile ilişkili verileri işlemek ve depolamak için gereken hesaplama ve depolama kaynaklarının miktarını ölçer. ComputationPrice\[\tau]$ ve $StoragePrice$ gas fiyatları sırasıyla hesaplama ve depolama maliyetlerini SUI birimlerine çevirir. SUI'nin piyasa fiyatı talep ve arz dalgalanmalarına bağlı olarak zaman içinde dalgalanacağından, gas birimleri ve gas fiyatları arasındaki ayrışma yararlıdır.

### Hesaplama gas fiyatları <a href="#computation-gas-prices" id="computation-gas-prices"></a>

Hesaplama gas fiyatı $ComputationPrice\[\tau]$, SUI birimleri cinsinden bir hesaplama biriminin maliyetini yakalar. Bu fiyat işlem düzeyinde belirlenir ve kullanıcı tarafından iki parça halinde sunulur:

$ComputationPrice\[\tau] \ = \ ReferencePrice \ + \ Tip\[\tau]$

$\text{with } \ ComputationPrice\[\tau] > PriceFloor$

ReferansFiyat$ dönem boyunca ağ seviyesinde sabit tutulurken, ipucu kullanıcının takdirine bağlıdır. İpucu negatif olabileceğinden, pratikte kullanıcı herhangi bir gas fiyatı gönderebilir - toplam fiyat $PriceFloor$ değerinden yüksek olduğu sürece. PriceFloor$'un $ReferansFiyat > PriceFloor$ olacak şekilde periyodik olarak güncelleneceğini ve esas olarak ağ spam'lerini önlemek için var olduğunu unutmayın.

Sui'nin gas fiyat mekanizması, $ReferansFiyat$'ı kullanıcıların ağa işlem gönderirken kullanabilecekleri güvenilir bir çapa haline getirmeyi ve böylece referans fiyatta veya referans fiyata yakın gas fiyatlarıyla gönderilen işlemlerin zamanında gerçekleştirileceğine dair makul bir güven sağlamayı amaçlamaktadır. Bu, üç temel adımla gerçekleştirilir:

* _Gas Fiyatı Anketi_ - Her dönemin başında validatör çapında bir anket yapılır ve her validatör kendi rezervasyon fiyatını bildirir. Yani, her bir validatör işlem yapmak istedikleri minimum gas fiyatını belirtir. Protokol bu teklifleri sıralar ve hisseye göre 2/3'lük yüzdelik dilimi referans fiyat olarak seçer. Gas fiyatı anketinin amacı, validatörlerin çoğunluğunun işlemleri derhal gerçekleştirmeye istekli olduğu bir referans fiyat belirlemektir.
* _Çetele Kuralı_ - Dönem boyunca, validatörler diğer validatörlerin işlemleri üzerinden sinyaller elde eder. Her bir validatör bu sinyalleri kullanarak diğer tüm validatörlerin performansı üzerine Spesifik olarak, her bir doğrulayıcı diğer tüm validatörlerin stake ödülleri için bir çarpan oluşturur, öyle ki iyi davranan validatörler artırılmış ödüller, iyi davranmayan validatörler ise azaltılmış ödüller alır. İyi davranış, bir validatörün kendi beyan ettiği rezervasyon fiyatının üzerinde olan ve validatörün zamanında işlediği işlemlerin payı ile gösterilir. Çeteleme kuralının amacı, validatörleri gas anketi sırasında sunulan fiyat tekliflerine uymaya teşvik etmek için topluluk tarafından uygulanan bir mekanizma oluşturmaktır.
* _Teşvik Edilmiş Hisse Ödülü Dağıtım Kuralı_ - Dönemin sonunda, stake ödüllerinin validatörler arasındaki dağılımı, gas fiyat araştırmasından ve çeteleme kuralından elde edilen bilgiler kullanılarak ayarlanır. Özellikle, çeteleme kuralı sırasında oluşturulan bireysel çarpanlar kümesinden - hisseye göre ağırlıklandırılmış - medyan değer kullanılarak her validatör için global bir çarpan oluşturulur. Teşvik edilmiş stake ödülü dağılımı daha sonra her bir validatöre dağıtılan stake ödüllerinin payını $v$ olarak belirler:

$ RewardShare(v) = Constant \times (1 + GasSurveyBoost) \times Multiplier(v) \times StakeShare(v) $

Sabit$ terimi, validatör seti genelinde $RewardShare(v)$ toplamı bir olacak şekilde bir normalleştirme olarak kullanılır. Validatör $ReferansFiyat$ altında bir fiyat teklifi sunmuşsa, $GasSurveyBoost > 0$ olur. Değilse, $GasSurveyBoost < 0$ olur. Bu güçlendiricinin amacı, validatörleri gas fiyatı anketi sırasında düşük rezervasyon fiyatları sunmaya teşvik etmektir. Son olarak, $Multiplier(v)$, çeteleme kuralındaki öznel değerlendirmelerden oluşturulan global çarpandır. Tüm validatörlerin gas fiyatı anketine aynı teklifi sunduğu ve tüm validatörlerin çeteleme kuralına göre iyi davrandığı simetrik bir dengede, $ RewardShare(v) = StakeShare(v)$ ve her validatörün toplam stake paylarıyla orantılı olarak stake ödülleri aldığını unutmayın.

Özetle, gas fiyatı mekanizmasının iki ana gücü vardır: sayım kuralı doğrulayıcıları gas anketi sırasında verilen tekliflere uymaya teşvik ederken, dağıtım kuralı doğrulayıcıları düşük rezervasyon fiyatları sunmaya teşvik eder. Bu iki gücün etkileşimi, validatörleri şebeke düzeyinde düşük bir referans gas fiyatı belirlemeye teşvik eden bir mekanizma sunar - ancak çok düşük olmamalıdır çünkü tekliflerini yerine getiremezlerse cezalarla karşılaşırlar. Başka bir deyişle, gas fiyatı mekanizması adil fiyatlar için sağlıklı bir rekabeti teşvik eder.

### Depolama gas fiyatları <a href="#storage-gas-prices" id="storage-gas-prices"></a>

Depolama gas fiyatı $StoragePrice$, bir birim depolamanın SUI birimleri cinsinden ebediyen karşılanmasının maliyetini ifade eder. Bu fiyat yönetişim teklifleri aracılığıyla belirlenir ve seyrek olarak güncellenir. Amaç, Sui kullanıcılarının bu ücretleri depolama fonuna yatırarak ve daha sonra bu ücretleri gelecekteki validatörlere yeniden dağıtarak zincir içi veri depolama kullanımları için ödeme yapmalarını sağlamaktır. Hesaplama gas fiyatının aksine, depolama fiyatları sabittir ve depolama fiyatı güncellenene kadar hem bir dönem içindeki hem de dönemler arasındaki tüm işlemler için ortaktır.

StoragePrice$, veri depolamanın zincir dışı dolar maliyetini hedeflemek amacıyla yönetişim teklifi aracılığıyla dışsal olarak belirlenir. Uzun vadede, teknolojik gelişmeler nedeniyle depolama maliyetleri düştükçe ve SUI tokeninin dolar fiyatı geliştikçe, yönetişim teklifleri yeni dolar hedef fiyatını yansıtmak için fiyatı güncelleyecektir.

### Bir koordinasyon mekanizması olarak gas fiyatları <a href="#gas-prices-as-a-coordination-mechanism" id="gas-prices-as-a-coordination-mechanism"></a>

Genel olarak, hesaplama gas fiyatları mevcut dönemin $ReferencePrice$ seviyesinde veya buna yakın olan ve depolama gas'i fiyatları hedeflenen $StoragePrice$ seviyesinde olan kullanıcılar iyi bir kullanıcı deneyimi yaşamaktadır. Sui'nin gas fiyatı mekanizması, son kullanıcılara işlemlerini göndermeleri için güvenilir referans fiyatlar sağlar. Validatörlerin gerçek rezervasyon fiyatlarını ortaya çıkarmaları ve bu fiyat tekliflerine uymaları için teşvik edilmeleri sayesinde kullanıcılar işlemlerinin zamanında işleme alınacağını güvenilir bir şekilde varsayabilirler.

Ağ aktivitesi arttığında, validatörler daha fazla işçi ekler, maliyetlerini doğrusal olarak artırır ve yine de düşük gas fiyatlarında işlem yapabilirler. Validatörlerin yeterince hızlı ölçeklenemediği aşırı ağ tıkanıklığı durumlarında, bahşişin varlığı, Sui platformunda işlem yapmanın maliyetini artırarak daha fazla talep artışını caydıran piyasa temelli bir tıkanıklık fiyatlandırma mekanizması sağlar.

Uzun vadede, Sui'nin gas mekanizması validatörlerin donanımlarını ve operasyonlarını optimize etmeleri için teşvikler yaratır. Daha verimli olmak için yatırım yapan validatörler, daha düşük gas fiyatlarını onurlandırabilir ve bir stake ödül artışı elde edebilirler. Sui validatörleri böylece yenilik yapmaya ve son kullanıcıların deneyimini iyileştirmeye teşvik edilir.
