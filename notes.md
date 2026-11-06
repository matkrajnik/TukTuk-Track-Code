TukTuk Track 2.5.0: Workflows (eksperymentalne) — procesy agentów odblokowywane kluczem.

- Nowy moduł Workflows: proces agentów (analiza, implementacja, review) w osobnym katalogu roboczym, z Twoją zgodą na konkretny diff. Bez automatycznego merge.
- Moduł włącza klucz dostępu w Ustawieniach → Workflows i można go tam wyłączyć; historia i definicje zostają.
- Problemy po awarii lub restarcie są widoczne w alercie, także na tablicy agentów.
- Claude Code 2.1.319+ może wykonywać kroki z zapisem (tryb `--restricted`: zmiany tylko w katalogu roboczym, bez poleceń powłoki).
- Bez klucza nic się nie zmienia: śledzenie czasu, raporty i integracje działają jak dotąd.
