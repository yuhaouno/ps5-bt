# Audio Bluetooth para PS5 — título provisional

Una pila de audio Bluetooth centrada en Rust para entornos homebrew de PS5,
con objetivos de baja latencia, calidad y portabilidad. La meta es conectar
auriculares normales mediante el Bluetooth integrado y conservar el DualSense
inalámbrico.

**Estado: investigación y estructura inicial de la fase 0. No hay un payload de
audio utilizable ni compatibilidad o latencia verificadas.** El adaptador C
devuelve explícitamente «no compatible». Las pruebas simuladas no demuestran
funcionamiento en PS5. Quedan pendientes el acceso compartido al controlador
Bluetooth y la revisión del arranque del SDK.

Es un proyecto independiente, no una traducción de código C a Rust. Respetaremos
las licencias y atribuciones al reutilizar adaptación de plataforma en C. No se
incluyen exploits, escalada de privilegios, evasión de DRM, binarios de Sony ni
firmware. Las contribuciones originales usan GPL-3.0-or-later.

Esta es una guía breve de estado. El [README en inglés](../../README.md) es la
referencia oficial y prevalece si esta traducción queda desactualizada. Consulte
allí la compilación, arquitectura, investigación y hoja de ruta. El nombre final
se decidirá después de comprobar posibles conflictos de repositorios y paquetes.
