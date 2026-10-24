# sysorbit — Türkçe.
#
# Ekler sayılara ve saatlere değil, sabit kelimelere bağlanır ("saat 03:00",
# "her saat 30 geçe", "1. günü"); böylece ünlü uyumu gerekmez.

## Header and status line

scope-system = Sistem
scope-user = Kullanıcı
zoom-hours = 6 saat
zoom-day = Gün
zoom-week = Hafta
zoom-month = Ay
refresh = Yenile
refreshing = Yenileniyor…
reading-timers = Zamanlayıcılar okunuyor…
no-timers = Bu kapsamda zamanlayıcı yok.
no-host = Sunucu yok
host-this-computer = bu bilgisayar
host-ssh-config = ~/.ssh/config'ten
host-add-hint = Sunucuları { $path } dosyasına ekleyin
no-hosts = Henüz sunucu yok. { $path } dosyasına bir tane ekleyin.
menu-add = + Sunucu ekle…
menu-edit = Düzenle
form-title-add = Sunucu ekle
form-title-edit = Sunucuyu düzenle
form-name = Ad
form-name-placeholder = Prod web
form-address = Sunucu
form-address-placeholder = 203.0.113.10 ya da ~/.ssh/config'teki bir ad
form-port = Port
form-user = Kullanıcı
form-user-placeholder = ~/.ssh/config'teki gibi
form-auth = Giriş yöntemi
form-auth-ssh = SSH ayarlarım (agent, ~/.ssh/config)
form-auth-key = Bir anahtar dosyası, örneğin PEM
form-key-placeholder = ~/keys/sunucu.pem
form-browse = Seç…
form-browse-title = Özel anahtar seçin
form-test = Bağlantıyı dene
form-testing = Deneniyor…
form-test-timeout = 20 saniye içinde yanıt gelmedi.
form-test-systemd = systemd { $version }
form-test-connected = bağlandı
form-test-journal = journal okunabiliyor
form-test-journal-restricted = journal yalnızca bu kullanıcının kayıtlarını gösteriyor; kullanıcıyı systemd-journal grubuna ekleyin
form-cancel = Vazgeç
form-save = Kaydet
form-delete = Sil
form-error-address = Bir sunucu girin.
form-error-port = Port 1 ile 65535 arasında bir sayı olmalı.
form-error-key = Bir anahtar dosyası seçin.
status-timers = { $count } zamanlayıcı · güncellendi { $time }
status-live = ● canlı
status-connecting = ○ bağlanıyor
status-polling = ○ { $seconds } sn'de bir yoklanıyor, canlı güncelleme yok: { $reason }
status-keys = ←/→ işaretler · ↑/↓ zamanlayıcılar · Enter loglar · Esc geri
live-closed = systemd bağlantısı kapandı

## Warnings from reading systemd

warning-old-systemd = systemd { $version }, desteklenen en eski sürümden ({ $minimum }) eski
warning-next-elapse-only = yalnızca bir sonraki tetiklenme gösteriliyor (systemd < 242)
warning-no-past-elapses = geçmiş tetiklenmeler gösterilmiyor (systemd < 244)
warning-monotonic = Monotonik zamanlar alınamadı: { $error }
warning-history = Geçmiş alınamadı: { $error }
warning-journal-restricted = Geçmiş alınamadı: journal yalnızca bu kullanıcının kayıtlarını gösteriyor. Kullanıcıyı systemd-journal grubuna ekleyin.
warning-downtime = Kapalı kalma süreleri alınamadı: { $error }
warning-clock = Sunucu saati okunamadı: { $error }
host-clock-synced = sunucu saati senkron
host-clock = sunucu saati { $direction ->
    [ahead] { $span } ileri
   *[behind] { $span } geri
}
host-zone = sunucu saati ({ $zone })
warning-unreadable = { $unit } okunamadı: { $error }

## Dates and durations

weekday-short-mon = Pzt
weekday-short-tue = Sal
weekday-short-wed = Çar
weekday-short-thu = Per
weekday-short-fri = Cum
weekday-short-sat = Cmt
weekday-short-sun = Paz
month-short-1 = Oca
month-short-2 = Şub
month-short-3 = Mar
month-short-4 = Nis
month-short-5 = May
month-short-6 = Haz
month-short-7 = Tem
month-short-8 = Ağu
month-short-9 = Eyl
month-short-10 = Eki
month-short-11 = Kas
month-short-12 = Ara
moment = { $day } { $month } { $weekday } { $time }
relative-now = şimdi
relative-future = { $span } sonra
relative-past = { $span } önce
short-week = { $n }hf
short-day = { $n }g
short-hour = { $n }sa
short-minute = { $n }dk
short-second = { $n }sn
short-millisecond = { $n }ms
short-zero = 0
span-week = { $n } hafta
span-day = { $n } gün
span-hour = { $n } saat
span-minute = { $n } dakika
span-second = { $n } saniye
span-millisecond = { $n } milisaniye
span-zero = 0 saniye
ordinal = { $n }.
list-separator = {", "}
list-and = { $rest } ve { $last }

## Track subtitles in the music box

schedule-daily = her gün { $time }
schedule-hourly = saatte bir
schedule-every-minute = dakikada bir
schedule-activation = etkinleşme +{ $offset }
schedule-boot = açılış +{ $offset }
schedule-startup = başlangıç +{ $offset }
schedule-every = her { $offset }
schedule-after-finish = bitişten { $offset } sonra
schedule-random = +{ $delay } rastgele
schedule-persistent = kalıcı
schedule-wake = Uyandırır
unit-file-state = { $state ->
    [enabled] etkin
    [enabled-runtime] etkin
    [disabled] devre dışı
    [static] statik
    [masked] maskeli
    [masked-runtime] maskeli
    [transient] geçici
    [generated] üretilmiş
    [indirect] dolaylı
    [linked] bağlı
    [linked-runtime] bağlı
    [alias] takma ad
   *[other] { $state }
}

## Music box marks and legend

now-clock = şimdi { $time }
mark-ran = Çalıştı · { $moment }
mark-took = süre { $span }
mark-still-running = hâlâ çalışıyor
outcome-success = başarılı
outcome-running = çalışıyor
outcome-failed = başarısız: { $result }
mark-planned = Planlandı · { $moment }
mark-random-picked = takvim { $moment }, +{ $span } rastgele gecikme
mark-random-window = { $moment } sonrasında { $span } içinde rastgele
mark-wakes = makineyi uykudan uyandırır
mark-ghost = zamanlayıcı etkin değil; etkin olsaydı burada tetiklenirdi
mark-skipped = Atlandı · { $moment }
mark-skipped-down = makine kapalıydı
mark-skipped-no-run = journal'da çalışma kaydı yok
mark-catch-up = Kaçırılanın telafisi (Persistent)
mark-tether = Monotonik geri sayım
downtime-powered-off = Kapalı
downtime-suspended = Uykuda
legend-ran = çalıştı
legend-failed = başarısız
legend-planned = planlandı
legend-delay = RandomizedDelaySec
legend-wake = WakeSystem
legend-skipped = atlandı
legend-catch-up = telafi (Persistent)
legend-down = makine kapalı

## Drawer

tab-overview = Genel bakış
tab-runs = Çalışmalar
tab-logs = Loglar
no-runs = Yüklenen aralıkta çalışma yok.
run-logs-link = loglar →
run-heading = Çalışma · { $moment }
no-invocation = systemd bu çalışma için bir invocation ID kaydetmedi.
logs-failed = Loglar okunamadı: { $error }
logs-empty = Bu çalışma journal'a hiçbir şey yazmadı.
logs-loading = Loglar yükleniyor…

## Overview tab

card-schedule = Takvim
card-next-run = Sonraki çalışma
card-recent-runs = Son çalışmalar
service-running = Servis şu anda çalışıyor.
service-idle = Servis çalışmıyor.
badge-active = etkin
badge-inactive = etkin değil
badge-persistent = kalıcı
badge-wake = uyandırır
help-active = Zamanlayıcı başlatılmış; süresi dolunca servisini tetikler.
help-inactive = Zamanlayıcı durdurulmuş; başlatılana kadar tetiklenmez.
help-persistent = Makine kapalıyken ya da uykudayken kaçırılan tetiklenme, makine geri gelince hemen çalışır.
help-wake = Süre dolunca makineyi uykudan uyandırır.
help-unit-file = { $state ->
    [enabled] Açılışta, kullanıcı zamanlayıcılarında oturum açılınca kendiliğinden başlar.
    [enabled-runtime] Açılışta, kullanıcı zamanlayıcılarında oturum açılınca kendiliğinden başlar.
    [disabled] Kurulu, ama yalnızca elle başlatılınca çalışır.
    [static] [Install] bölümü yok, etkinleştirilemez; onu başka bir unit başlatır.
    [masked] systemd başlatmayı reddeder.
    [masked-runtime] systemd başlatmayı reddeder.
    [transient] Çalışma anında oluşturulmuş (ör. systemd-run ile); yeniden başlatınca kaybolur.
    [generated] Başka bir yapılandırmadan bir generator tarafından oluşturulmuş.
    [indirect] Başka bir unit'in Also= ayarıyla etkinleştirilmiş.
    [linked] Unit dosyası arama yolunun dışında duruyor.
    [linked-runtime] Unit dosyası arama yolunun dışında duruyor.
    [alias] Başka bir unit'in diğer adı.
   *[other] systemd'nin bildirdiği unit dosyası durumu.
}
no-triggers = Tetikleyici yok.
next-occurrences = Sonraki: { $moments }
running-since = Şu anda çalışıyor, { $time } saatinden beri.
timer-inactive-note = Zamanlayıcı etkin değil; başlatılana kadar tetiklenmez.
no-upcoming = systemd'de bu zamanlayıcı için yaklaşan bir tetiklenme yok.
accuracy-note = En fazla { $span } gecikebilir (AccuracySec={ $value }): systemd uyanmaları bu aralıkta toplar.
delay-note = Her tetiklenmeden sonra 0 ile { $span } arası rastgele bekler (RandomizedDelaySec={ $value }).
bar-last = son { $moment } · { $relative }
bar-next = sonraki { $moment } · { $relative }
bar-not-scheduled = planlanmamış
stat-succeeded = { $count } başarılı
stat-failed = { $count } başarısız
stat-average = ort. { $span }
stat-p95 = p95 { $span }
durations-not-logged = süreler kaydedilmemiş
duration-not-logged = süre kaydedilmemiş
chart-summary = son { $count } · en uzun { $span }
chart-run = { $moment } · { $span } · { $outcome }

## Triggers in plain words

explain-unknown = Bu ifadenin sade bir karşılığı yok.
weekdays-mon = Pazartesi
weekdays-tue = Salı
weekdays-wed = Çarşamba
weekdays-thu = Perşembe
weekdays-fri = Cuma
weekdays-sat = Cumartesi
weekdays-sun = Pazar
weekday-mon = Pazartesi
weekday-tue = Salı
weekday-wed = Çarşamba
weekday-thu = Perşembe
weekday-fri = Cuma
weekday-sat = Cumartesi
weekday-sun = Pazar
month-1 = Ocak
month-2 = Şubat
month-3 = Mart
month-4 = Nisan
month-5 = Mayıs
month-6 = Haziran
month-7 = Temmuz
month-8 = Ağustos
month-9 = Eylül
month-10 = Ekim
month-11 = Kasım
month-12 = Aralık
nth-1 = ilk
nth-2 = ikinci
nth-3 = üçüncü
nth-4 = dördüncü
nth-last = son

explain-at = saat { $times }
explain-every-second = her saniye
explain-every-n-seconds = { $n } saniyede bir
explain-every-minute-at-second = her dakikanın { $second }. saniyesinde
explain-fields = saat { $hours }, dakika { $minutes } ve saniye { $seconds } olduğunda
explain-any = herhangi
explain-every-from-to = { $from }–{ $to } arası, { $step } aralıkla
minute-mark = { $minute }
explain-every-minute = dakikada bir
explain-every-hour = her saat başı
explain-every-hour-at = her saat { $minute } geçe
explain-every-n-minutes = { $n } dakikada bir
explain-every-n-minutes-from = her saat { $minute } geçeden başlayarak { $n } dakikada bir
explain-every-minute-between = her saat { $from } ile { $to } geçe arasında dakikada bir
explain-every-n-minutes-between = her saat { $from } ile { $to } geçe arasında { $n } dakikada bir
explain-past-the-hour = her saat { $minutes } geçe
explain-every-hour-range = { $from } ile { $to } arasında saatte bir
explain-every-n-hours = { $n } saatte bir
explain-every-n-hours-at = { $n } saatte bir, { $minute } geçe
explain-every-n-hours-range = { $from } ile { $to } arasında { $n } saatte bir
explain-past-hours = { $hours } saatlerinde { $minute } geçe
explain-between = { $from } ile { $to } arasında
explain-during-hours = { $hours } saatlerinde
explain-repeat-window = { $window } { $repeat }

explain-on-weekdays = hafta içi
explain-on-weekends = hafta sonu
explain-weekday-range = { $from }–{ $to } günleri
explain-on-days = { $days } günleri
explain-nth-weekday = { $months } { $nth } { $weekday } günü
explain-every-month = her ayın
explain-months-of = { $count ->
    [one] { $months } ayının
   *[other] { $months } aylarının
}
explain-in-months = { $count ->
    [one] { $months } ayında
   *[other] { $months } aylarında
}
explain-month-range = { $from }–{ $to } arasında
explain-date = { $day } { $month } günü
explain-day-of = { $months } { $day }
explain-day-one = { $day } günü
explain-day-range = { $from } ile { $to } günleri arasında
explain-odd-days = tek günlerinde
explain-even-days = çift günlerinde
explain-every-n-days = { $from } gününden başlayarak { $n } günde bir
explain-day-list = { $days } günleri
explain-last-day = son günü
explain-nth-last-day = sondan { $nth } günü
explain-last-n-days = son { $n } gününde
explain-days-from-end = sondan { $days } günlerinde
explain-in-year = { $year } yılında
explain-year-range = { $from }–{ $to } yılları arasında
explain-every-n-years = { $from } yılından başlayarak { $n } yılda bir
explain-in-years = { $years } yıllarında

explain-daily = her gün, { $time }
explain-dated = { $date }, { $time }
explain-repeat-dated = { $date }, { $repeat }
explain-zone = { $sentence } ({ $zone } saatiyle)

explain-after-activation = Zamanlayıcı başlatıldıktan { $span } sonra
explain-after-boot = Açılıştan { $span } sonra
explain-after-startup = Servis yöneticisi başladıktan { $span } sonra (kullanıcı zamanlayıcılarında: oturum açıldıktan)
explain-after-unit-active = Servis her başladıktan { $span } sonra yeniden
explain-after-unit-inactive = Servis her bitişinden { $span } sonra


new-timer = Yeni zamanlayıcı
timer-form-title = Yeni zamanlayıcı
timer-form-name = Ad
timer-form-name-placeholder = yedekleme
timer-form-description = Açıklama
timer-form-description-placeholder = Belgelerimi yedekle
timer-form-command = Komut
timer-form-command-placeholder = /usr/bin/rsync -a /home/ben/belgeler /mnt/yedek
timer-form-command-hint = Yazıldığı gibi, kabuk olmadan çalışır. Boru veya && için /bin/sh -c '…' kullanın.
timer-form-calendar = Zamanlama
timer-form-calendar-placeholder = daily
timer-form-persistent = Bilgisayar kapalıyken kaçırılan çalıştırmayı sonradan yap
timer-form-scope = Kurulacak yer
timer-form-note-user = Sizin kullanıcınızla çalışır. Linger açık değilse (loginctl enable-linger) yalnızca oturumunuz açıkken çalışır.
timer-form-note-system-local = root olarak çalışır. Oluştururken yönetici parolası sorulur.
timer-form-note-system-remote = root olarak çalışır. SSH kullanıcısı root olmalı ya da parolasız sudo kullanabilmeli.
timer-form-create = Oluştur
timer-form-creating = Oluşturuluyor…
timer-form-error-name-empty = Bir ad girin.
timer-form-error-name = Adda yalnızca harf, rakam ve : - _ . kullanın.
timer-form-error-command = Bir komut girin.
timer-form-error-calendar = Bir zamanlama girin.
timer-form-error-line = Her alan tek satıra sığmalı.
timer-form-error-exists = { $name } zaten var. Başka bir ad seçin.
timer-form-error-password = sudo parola istedi; SSH üzerinden parola girilemez. Bu kullanıcıya parolasız sudo izni verin ya da root olarak bağlanın.
composer-untitled = Yeni zamanlayıcı
composer-no-schedule = Ne zaman çalışacağını görmek için birkaç saat boyayın
composer-checking = systemd'ye soruluyor…
composer-invalid = systemd bu zamanlamayı anlamıyor
composer-when = Ne zaman çalışır
composer-none-in-view = Gösterilen aralıkta çalışma yok; sonrakileri görmek için uzaklaştırın.
composer-show-files = Unit dosyalarını göster
composer-hide-files = Unit dosyalarını gizle
rhythm-mode-week = Hafta
rhythm-mode-month = Ayın günleri
rhythm-mode-advanced = Elle
rhythm-preset-hourly = Her saat
rhythm-preset-quarterly = Her 15 dk
rhythm-preset-nightly = Her gece 03
rhythm-preset-workdays = Hafta içi 09
rhythm-preset-weekly = Pazar 02
rhythm-grid-hint = Saatleri boyamak için tıklayın ya da sürükleyin; gün adı veya saat tüm satırı/sütunu doldurur.
rhythm-per-hour = Saatte
rhythm-minute = Başlangıç dakikası
rhythm-month-days = Günler
rhythm-month-hours = Saatler
rhythm-last-day = Son gün
rhythm-advanced-hint = OnCalendar ifadeleri; örneğin Mon..Fri 09:00 veya *-*-01 03:00. Birden çoğunu ; ile ayırın.
rhythm-card-title = systemd'nin okuyacağı
composer-summary-title = Ne olacak
composer-summary-command = { $name }, { $command } komutunu çalıştırır.
composer-summary-no-command = { $name } için henüz komut yok.
composer-summary-no-schedule = Henüz zaman seçilmedi.
composer-summary-checking = Ne zaman çalışacağı systemd'ye soruluyor…
composer-summary-invalid = systemd bu zamanlamayı henüz anlamıyor.
composer-summary-system = Tüm sistem için, root olarak çalışır.
composer-summary-user = Sizin kullanıcınızla çalışır.
composer-summary-persistent = Kaçırılan çalıştırmalar bir sonraki açılışta yapılır.
composer-summary-next = Sonraki çalışma: { $moment }, { $relative }.
composer-step-what = Ne çalışsın
composer-step-what-hint = Zamanlayıcının başlatacağı komut; aynı adlı bir servis üzerinden çalışır.
composer-step-when = Ne zaman
composer-step-when-hint = Zamanları seçin; yukarıdaki yeşil track bunları diğer zamanlayıcılarınızın arasında gösterir.
composer-step-how = Nasıl ve nereye
composer-step-how-hint = Kaçırılan çalıştırmalara ne olacağı ve komutu kimin çalıştıracağı.
composer-step-create = Kontrol et ve oluştur
composer-step-create-hint = sysorbit aşağıdaki iki dosyayı yazar, ardından zamanlayıcıyı etkinleştirip başlatır.
composer-persistent-on = Bilgisayar kapalı ya da uykudayken kaçırılan çalıştırma, yeniden açılınca bir kez yapılır.
composer-persistent-off = Bilgisayar kapalıyken denk gelen çalıştırmalar atlanır.
rhythm-presets = Hızlı başlangıç
rhythm-month-hint = Ayın günlerini ve o günlerde çalışacağı saatleri seçin.
rhythm-hint-cell-off = Eklemek için tıklayın: her { $days } { $times } saatinde çalışır.
rhythm-hint-cell-on = Boyalı: her { $days } { $times } saatinde çalışır. Kaldırmak için tıklayın.
rhythm-hint-day = Bu günün ({ $days }) tüm saatlerini doldurur; hepsi boyalıysa temizler.
rhythm-hint-hour = { $hour } saatini her gün için doldurur; tüm günlerde boyalıysa temizler.
rhythm-hint-divisions = Boyalı her saatin kaç kez çalışacağı. Boyalı 09 şu an { $times } saatlerinde çalışır.
rhythm-hint-minute = Çalışmaları saat içinde kaydırır. Boyalı 09 şu an { $times } saatlerinde çalışır.
rhythm-hint-preset = Izgarayı şununla değiştirir: { $schedule }.
rhythm-hint-modes = Hafta: haftanın saatlerini boyayın. Ayın günleri: ayın 1'i, 15'i, son günü… Elle: systemd ifadelerini kendiniz yazın.
rhythm-hint-month-day = Her ayın { $day }. gününde çalışır.
rhythm-hint-month-day-late = Her ayın { $day }. gününde çalışır; bu günü olmayan aylarda atlanır. Ay sonları için “Son gün”ü kullanın.
rhythm-hint-last-day = Her ayın son gününde çalışır; 28'i de olsa 31'i de.
rhythm-hint-month-hour = Seçili günlerde { $time } saatinde çalışır.
rhythm-hint-month-minute = Seçili her saatin hangi dakikasında çalışacağı.
info-oncalendar = OnCalendar= zamanlayıcının takvim ayarıdır. sysorbit bunu seçtiklerinizden yazar; birden çok satır varsa zamanlayıcı hepsinde çalışır.
info-persistent = Persistent=true ile systemd son çalışmayı diske kaydeder. Bilgisayar kapalı ya da uykudayken bir çalışma kaçarsa, zamanlayıcı yeniden başlayınca bir kez çalışır.
info-scope = Sistem zamanlayıcıları /etc/systemd/system içinde durur ve kim oturum açmış olursa olsun root olarak çalışır. Kullanıcı zamanlayıcıları ~/.config/systemd/user içinde durur ve sizin adınıza, linger açık değilse (loginctl enable-linger) yalnızca oturumunuz açıkken çalışır.
