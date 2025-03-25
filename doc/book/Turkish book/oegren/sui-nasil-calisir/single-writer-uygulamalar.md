---
description: Sui Single-Writer-Friendly (SWF) Uygulamaları Kullanın
---

# Single-Writer Uygulamalar

Bu sayfada, Sui'de basit işlemler olarak tanımlanan tek yazarlı modelde çalışabilen uygulamalar listelenmektedir. Bariz tek yazarlı uygulamaların (basit eşler arası varlık transferi gibi) yanı sıra, tipik olarak paylaşılan nesneler gerektiren bazı tekliflerin, oylama ve piyangolar ve DeFi Oracle fiyat teklifleri gönderimi gibi her eylem için değil, yalnızca son adım olarak paylaşılan bir nesne gerektiren varyantlara dönüştürüldüğüne dikkat edin.



1. Düzenli eşler arası (p2p) işlemler ([sadece 7 satır Sui Move kodu ile yeni bir Coin'in nasıl oluşturulacağına bakın](https://www.linkedin.com/posts/chalkiaskostas\_startup-smartcontract-cryptocurrency-activity-6946006856528003072-CvI0)).
2. Gizli p2p Tx'ler: FastPay ile aynıdır, ancak transfer edilen miktarları gizlemek için pedersen taahhütleri vardır; bu hala girdi miktarı = çıktı miktarını sağlar - miktar sınırlarını belirleyebiliriz, yani 1.000 $ 'a kadar N transferleri gizli olabilir.
3. Herkese açık ilan panosu; kullanıcılar yalnızca herkese açık olarak erişilen verileri, dosyaları, bağlantıları, meta verileri depolar.
4. Varlık kanıtı: yukarıdakine benzer, ancak zaman damgalı belgeler için; varlığın taahhüt kanıtını destekleyecek şekilde genişletilebilir, yani hash'inizi yayınlayın, sonra ifşa edin.
5. Özel merkezi olmayan depo (kullanıcılar kendi açık anahtarları altında şifrelenmiş özel dosyaları depolar; kullanıcıların açık anahtarları NFT'ler olarak temsil edilebilir.
6. Seçilen açıklama CV (özgeçmiş) deposu, Üniversite dereceleri deposu için yukarıdakileri genişletin.
7. Merkezi olmayan veya geleneksel Sertifika Yetkilisi. Yetkililer imzalarını sertifikalar üzerinden yayınlar, istedikleri zaman iptal edebilirler (daha kolay iptal).
8. Mesajlaşma hizmeti: mesaj alışverişi yapan uygulamalar, Oracle'lar ve Nesnelerin İnterneti (IoT'ler). Sui muhtemelen herhangi bir mesajlaşma protokolü için en iyi platformdur, çünkü tipik olarak her yanıt ve mesaj tek yazarlı bir NFT ile kodlanabilir.
9. Yukarıdakileri sosyal ağlara genişletin; her gönderinin tek yazarlı bir NFT olduğunu unutmayın. [Sadece 50 satır Sui Move kodu ile tamamen işlevsel, merkezi olmayan bir Twitter'ın akıllı kontrat uygulamasını görün](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/nfts/sources/chat.move).
10. Yukarıdakileri özel mesajlaşmaya genişletin (yani, merkezi olmayan WhatsApp veya Signal).
11. &#x20;Yukarıdakileri herhangi bir web sitesi / blog / derecelendirme platformu (örn. Yelp ve Tripadvisor) için genişletin.&#x20;
12. Kişisel GitHub, Overleaf LaTex editörü, istek/alışveriş listeleri vb.&#x20;
13. Kişisel şifre yöneticisi.
14. İnteraktif olmayan oyunlar (yani, SimCity, FarmVille durumunuzun reklamını yapın/geliştirin vb.)
15. İnsan ve Bilgisayar oyunları (örneğin, akıllı sözleşmeye programlanmış satranç yapay zekası. AI, kullanıcının satranç hamlesinin aynı işleminde otomatik olarak geri oynar).
16. Kuponlar ve biletler.&#x20;
17. Oyun varlıklarının toplu basımı.&#x20;
18. İyimser merkezi olmayan piyango: kazananı ilan etmek için yalnızca paylaşılan nesnelere ihtiyaç duyan ancak bilet satın almaya ihtiyaç duymayan yeni bir varyant; böylece milyon akıştan yalnızca birinin konsensüse ihtiyacı vardır.
19. Oylama için de aynı şey geçerlidir (her oy bir NFT'dir) - sadece sondaki toplama kısmının paylaşılan nesnelerle sahtekarlık kanıtlarını desteklemesi veya bunun uygulama katmanında gerçekleşmesi gerekir.
20. Çoğu açık artırma türü için aynıdır (her teklif bir NFT'dir) - bir kazananın ilan edilmesi sahtekarlık kanıtlarıyla sorgulanabilir; bu nedenle, paylaşılan bir nesne gerektiren tek adımdır.
21. Gelecekte hediye kartlarının şifresinin çözülmesi de dahil olmak üzere zamanlanmış şifreli mesajlar.
22. Fiyat tekliflerinin gönderilmesi (yani Oracles, Pyth, vb.'den) tek yazarlı olabilir ve bir DEX işlemi paylaşılan nesneleri kullanabilir. Yani Oracles %100 tek yazarlı modelde çalışabilir.
23. İş listesi ve ilgili uygulamalar (yani, merkezi olmayan bir Workable).
24. Gayrimenkul sözleşmesi deposu: yalnızca izleme amaçlıdır - ödeme çevrimdışıdır, aksi takdirde atomik bir takas olur.

Son güncelleme 8/15/2022, 5:29:02 PM

* [Kaynak Kodu](https://github.com/MystenLabs/sui/blob/devnet/doc/src/learn/single-writer-apps.md)
