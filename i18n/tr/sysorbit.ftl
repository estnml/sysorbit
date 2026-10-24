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
