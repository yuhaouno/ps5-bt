# Bluetooth-Audio für PS5 — Arbeitstitel

Ein auf Rust ausgerichteter Bluetooth-Audio-Stack für PS5-Homebrew mit dem Ziel
geringer Latenz, hoher Qualität und Portabilität. Normale Kopfhörer sollen das
integrierte Bluetooth nutzen, während der DualSense drahtlos weiterarbeitet.

**Status: Recherche und Grundgerüst aus Phase 0. Es gibt noch kein nutzbares
Audio-Payload und keine verifizierte Kompatibilität oder Latenzmessung.** Der
C-Adapter meldet ausdrücklich „nicht unterstützt“. Simulierte Tests belegen
keine Funktion auf der PS5. Der gemeinsame Zugriff auf den Bluetooth-Controller
und das Startverhalten des SDK müssen noch geprüft werden.

Dies ist ein unabhängiges Projekt, keine Übersetzung eines C-Projekts nach Rust.
Bewährte Plattformanpassungen in C können unter Beachtung ihrer Lizenzen und
Urheberhinweise genutzt werden. Exploits, Rechteausweitung, DRM-Umgehung,
Sony-Binärdateien und Firmware gehören nicht zum Projekt. Eigene Beiträge stehen
unter GPL-3.0-or-later.

Dies ist eine kurze Statusübersetzung. Maßgeblich ist das
[englische README](../../README.md), auch wenn diese Übersetzung veraltet ist.
Dort stehen Build-Anleitung, Architektur, Recherche und Roadmap. Der endgültige
Name wird erst nach Prüfung auf Repository- und Paketnamenskonflikte gewählt.
