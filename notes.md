Todo Track 1.9.0: Redmine obok Jiry.

- Nowa sekcja Ustawienia → Redmine. Połączenie z własnym Redmine przez adres (także z prefiksem ścieżki, np. https://firma.pl/redmine) i klucz API z „Moje konto”. Klucz jest przechowywany w Menedżerze poświadczeń Windows. Są też przyciski „Testuj połączenie” i „Rozłącz”. Aplikacja ufa certyfikatom z magazynu Windows; w „Zaawansowane” jest opcja dla niezaufanego certyfikatu w sieci wewnętrznej.
- Nowy widok Redmine. Wybierasz śledzone projekty, a zgłoszenia każdego z nich są pogrupowane według statusu. Do tego zakres „Moje” albo „Wszystkie otwarte”, filtr trackera, wyszukiwanie, „Rozpocznij” i „Otwórz w Redmine”. Na dole jest sekcja zgłoszeń przypisanych do Ciebie spoza śledzonych projektów.
- Co 5 minut nowe zgłoszenia przypisane do Ciebie trafiają do „Do zaplanowania” i pojawia się powiadomienie. Odświeżają się też statusy na kafelkach. Pierwsze sprawdzenie niczego nie importuje.
- Kafelek zadania pokazuje #1234, status, tracker i termin. Szybkie dodawanie rozumie „#1234”, gdy Redmine jest połączony. Zadanie może mieć jedno zgłoszenie: z Jiry albo z Redmine.
- Start pracy nad zgłoszeniem w statusie początkowym pyta, czy przesunąć je do „In Progress” (nazwę ustawisz w Ustawieniach). Kafelek i karta w widoku mają też „Przesuń do…”, „Przypisz do mnie”, „Dodaj komentarz” i „Kopiuj nazwę gałęzi”. Do Redmine nic nie zapisuje się bez Twojego kliknięcia, a zapisy wyłącza jeden przełącznik.
- Jira i Redmine działają niezależnie. Można połączyć jedno, oba albo żadne.

Integracja nie była jeszcze sprawdzana z prawdziwym serwerem Redmine. Przy polskich nazwach statusów (np. „Nowy”, „W toku”) wpisz je w Ustawienia → Redmine; pomoże przycisk „Wstaw statusy z Redmine”.
