# CrowdRelay — plan sprzedaży (labels · festiwale · managementy)

**North star:** większy crowd na koncertach i festiwalach + wyższa sprzedaż merchu, albumów i biletów. Wszystko poniżej temu służy.

## 1. Pozycjonowanie

Nie sprzedajemy „narzędzia do newslettera". Sprzedajemy **platformę pozyskiwania fanów**, na której:
- **LEWA (źródła):** podpinacie konta platform (Meta Lead Ads, TikTok, Google Lead Forms, Reddit, Bandsintown, CSV/n8n) — każde źródło to wymienny blok.
- **ŚRODEK (agent):** deterministyczny agent buduje **fanbazy** (bloki publiczności), prowadzi kampanie contentowe i reklamowe, testuje lejki i fanouty, i sam optymalizuje — zawsze w granicach uprawnień, które mu wyznaczycie (authority funnel + kill switch).
- **PRAWA (rezultat):** zweryfikowani double-opt-in fani w waszej bazie → bilety, merch, albumy → **crowd na miejscu**. Po każdym gigu raport wyniku (frekwencja vs target, $ vs target) wraca do systemu i podbija jakość następnych decyzji.

Różnica wobec konkurencji: oni sprzedają narzędzia. My sprzedajemy **proces, który sam się wykonuje i sam się mierzy** — plus cross-amplifikację między artystami rosteru (headliner ogrza nowego), czego nie da żaden punktowy produkt.

## 2. Segmenty i ICP

| Segment | Kto | Ból | Co kupuje |
|---|---|---|---|
| Festiwale/organizatorzy | 5–100k osób, wiele scen | zasięg lineupu nie zamienia się w bazę; brak atrybucji biletów | Festival Mode |
| Wytwórnie z rostrem | 10–200 artystów | koszt pozyskania fana rośnie; nowi artyści startują od zera | Roster + cross-amplifikacja |
| Managementy/agencje | 3–30 artystów | ręczna robota marketingowa, brak miary | Pilot → Roster |

## 3. Pakiety (ceny propozycyjne — do walidacji rynkowej)

### Pilot Connect — wejście (30 dni)
- import istniejącej listy (pending + double opt-in), 1–2 fanbazy, dashboard KPI, raport konwersji dzień zero → dzień 30.
- **Cena:** 6–12 tys. PLN setup + 1,5–3 tys. PLN/mies. przez czas pilota.
- Deliverable gwarantowany: liczba potwierdzonych fanów i pełna atrybucja źródła.

### Roster — core dla wytwórni/managementu
- pełny agent per artysta + **cross-amplifikacja**: headliner karmi fanbazą nowego signee'a,
- kampanie content+ads, limity i cooldowny per fanbase, audyt akcji.
- **Cena:** 600–1 500 PLN/artysta/mies. (min. 5 artystów), rabat rosnący z rozmiarem rosteru.

### Festival Mode — organizatorzy
- lineup-driven cross-promo między wykonawcami/scenami, sprzedaż biletów z atrybucją per fanbase i treść, po edycji: raport wyniku → baza zostaje u klienta.
- **Cena:** 25–60 tys. PLN za edycję + performance fee od trackowanej sprzedaży.

### Barter (ścieżka alternatywna)
Sloty dla Viryi (1–2 występy) ↔ wdrożenie platformy na społeczność organizatora. Zapisywany w systemie jako krawędź `event_crossbill` — deal i produkt mówią tym samym językiem.

## 4. Pipeline i materiały

Etapy: **Research → Personalizowana oferta (PDF) → Demo live (mapa + panel) → Pilot 30 dni → Kontrakt Roster/Festival.**

Materiały: `system-map.html` (grafika procesu), oferty PDF per klient, `clients-pipeline.csv` (statusy), ten dokument.

Skrypt demo (15 min): mapa procesu (2 min) → panel Portfolio na danych demo (4 min) → ingest na żywo z pasty JSON (3 min) → raport atrybucji i case-study export (3 min) → pytania (3 min).

## 5. Obiekcje i odpowiedzi

- **„To AI będzie spamować naszych fanów?"** — Nie. Treść przechodzi przegląd człowieka przed publikacją (tryb reviewed), dopiero po dobrych wynikach operator dostroić może tryb. Każda akcja jest rejestrowana.
- **„RODO?"** — Double opt-in, wypisani nigdy nie wracają, fani nie opuszczają workspace'u właściciela, pełne logi.
- **„Mamy już CRM/mailing."** — My dowozimy *nowych* fanów z platform i atrybucję sprzedaży; mailing zostaje, dostaje lepsze segmenty.
- **„Dlaczego wy?"** — Działający produkt (pierwszy tenant live: Virya), deterministyczny silnik decyzji z limitami, zero black boxa.

## 6. Metryki sukcesu (case study Virya — tenant #1)

Po 60–90 dniach publikujemy: pozyskani potwierdzeni fani/mies., koszt per fana vs reklamy manualne, % fanów z atrybucją źródła, wzrost sprzedaży biletów/merchu z trackowanych linków. To jest nasz argument do kolejnych rozmów.

## 7. Proces personalizacji oferty

1. Uzupełnij `clients-pipeline.csv` (nazwa, segment, tier, status).
2. Skopiuj szablon `offers/roster-offer-template.html` → zmień blok `CONFIG` na górze pliku (nazwa klienta, widełki ceny, kontakt). Oferta festivalowa: `offers/mystic-coalition-offer.html` jako baza do personalizacji.
3. `python3 render.py offers/<plik>.html` → powstaje PDF.
4. Zapisz w pipeline `next_step` i termin follow-up (7 dni).
