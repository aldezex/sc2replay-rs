# Plan de proyecto: Port de sc2reader a Rust

**Objetivo del proyecto:** aprender Rust construyendo un parser de replays de StarCraft II funcionalmente equivalente a sc2reader (Python), validado campo a campo contra la librería original.

**Principio guía:** cada milestone debe ser ejecutable y verificable de forma independiente. No se avanza al siguiente milestone sin tener un test que compare tu output contra `sc2reader.load_replay()`.

---

## Fase 0 — Preparación (antes de escribir Rust)

**M0.1 — Entorno y fixtures**
- Instalar toolchain de Rust (rustup, cargo, rust-analyzer en tu editor).
- Crear el crate: `cargo new sc2reader-rs --lib` (+ un binario auxiliar en `src/bin/` para pruebas manuales).
- Reunir 5-10 replays de referencia variados: distintas razas, distintas versiones del juego, 1v1 y 2v2, alguno con desconexión/abandono si tienes.
- Instalar sc2reader en Python en un entorno separado — este será tu "oráculo" (fuente de verdad) durante todo el proyecto.

**M0.2 — Estudiar la especificación de facto**
- Leer el código fuente de sc2reader (no solo la doc): `resources.py`, `objects.py`, `events/*.py`.
- Leer el repo oficial `Blizzard/s2protocol` para entender el formato de serialización real.
- Escribir un documento propio (markdown) resumiendo: qué archivos internos tiene un `.SC2Replay`, qué contiene cada uno, en qué orden se procesan en sc2reader.
- *Criterio de éxito*: puedes explicar de memoria qué es `replay.details` vs `replay.tracker.events` vs `replay.game.events` sin mirar el código.

**M0.3 — Diseño de arquitectura del crate**
- Decidir estructura de módulos: `mpq/`, `protocol/`, `events/`, `domain/` (o similar).
- Decidir manejo de errores desde el principio (crate `thiserror` o `anyhow`, tipo `Result<T, Sc2ReplayError>` propio).
- Decidir estrategia de testing: snapshot testing (comparar contra JSON exportado desde Python) para cada milestone.

---

## Fase 1 — Contenedor MPQ

**M1.1 — Leer la estructura del archivo**
- Entender el formato MPQ (header, hash table, block table) a nivel conceptual.
- Decisión clave de aprendizaje: **implementar tu propio parser MPQ mínimo** (aunque exista un crate ya hecho) si el objetivo es aprender parsing binario en Rust — es donde más se aprende sobre `byteorder`, slices, lectura de structs binarias. Si prefieres enfocar el tiempo en la parte específica de SC2, usa un crate MPQ existente y anota esa decisión como consciente.

**M1.2 — Listar archivos internos**
- Dado un `.SC2Replay`, extraer y listar los nombres de los sub-archivos (`replay.details`, `replay.initData`, `replay.tracker.events`, `replay.game.events`, `replay.message.events`, `replay.attributes.events`).
- *Test*: comparar la lista de archivos contra lo que ves al inspeccionar el replay con herramientas Python/MPQ existentes.

**M1.3 — Extraer bytes crudos de cada sub-archivo**
- Descomprimir (los archivos MPQ suelen usar compresión zlib/bzip2 por bloque) y obtener los bytes crudos de al menos `replay.details` e `replay.initData`.
- *Test*: longitud en bytes y primeros N bytes coinciden con lo extraído manualmente en Python (`mpyq` o similar).

---

## Fase 2 — Deserialización del protocolo

**M2.1 — Entender el formato de serialización versionado**
- El protocolo de Blizzard varía según la versión del build del juego — cada versión tiene su propio "protocol module" con definiciones de structs.
- Documentar (para ti) cómo sc2reader/s2protocol seleccionan qué definición de protocolo usar según la versión del replay.

**M2.2 — Parsear `replay.details`**
- Implementar el decoder para esta estructura: mapa, jugadores, duración, fecha, resultado.
- Modelar en Rust: `struct ReplayDetails { map_name: String, players: Vec<PlayerDetails>, ... }`.
- *Test*: comparar campo a campo contra `replay.map_name`, `replay.players[i].name`, etc. en Python.

**M2.3 — Parsear `replay.initData`**
- Configuración de lobby, versión del juego, región/gateway.
- *Test*: comparar contra `replay.versions`, `replay.region`, etc.

**M2.4 — Parsear `replay.tracker.events`**
- Este es el archivo con más valor para tus métricas: creación/muerte de unidades, transferencias de recursos, stats periódicos.
- Implementar el decoder evento por evento (son varios tipos de eventos con distinto payload).
- *Test*: contar eventos por tipo y comparar contra Python; luego comparar los primeros 20 eventos byte a byte en campos clave (timestamp, unit_tag_index, etc.).

**M2.5 — Parsear `replay.game.events`**
- Comandos de jugador: selección, hotkeys, órdenes (build/train/attack/move).
- Es el archivo más denso y con más tipos de eventos — déjalo para cuando ya domines el patrón de los anteriores.
- *Test*: igual que M2.4, comparación de conteos y luego de campos.

**M2.6 — Parsear `replay.message.events` y `replay.attributes.events`**
- Chat, pings, y atributos de la partida (game mode, opciones de lobby).
- Menor prioridad para tu objetivo final (análisis de gameplay), pero completa el "1:1".

---

## Fase 3 — Capa de dominio (la "personalidad" de sc2reader)

**M3.1 — Modelar los tipos de dominio**
- Traducir a Rust las clases principales: `Replay`, `Player`, `Team`, `Unit`, `BuildEntry`.
- Decisiones idiomáticas de Rust a tomar aquí (parte importante del aprendizaje):
  - `enum Race { Terran, Protoss, Zerg, Random }` en vez de strings.
  - `Option<T>` donde Python usa `None`.
  - Lifetimes o `Rc`/`Arc` si hay referencias cruzadas entre `Unit` y `Player` (esto es un reto típico al portar grafos de objetos de Python a Rust — anticípalo).

**M3.2 — Ensamblar `Replay` a partir de las partes parseadas**
- Unir details + initData + tracker events + game events en el objeto `Replay` final.
- *Test*: `replay.players[0].race`, `replay.length`, `replay.winner` coinciden con Python.

**M3.3 — Reconstruir el build order**
- Derivar la lista de "qué se construyó/entrenó y cuándo" a partir de eventos de comandos + tracker events (esto es lógica derivada, no viene como un evento único).
- *Test*: comparar build order completo (primeros 15 min) de 3-5 replays contra `sc2reader`.

**M3.4 — Reconstruir estados de unidades**
- Timestamps de nacimiento/muerte, posiciones si están disponibles.
- *Test*: comparar conteo de unidades muertas por jugador y sus timestamps.

---

## Fase 4 — Datapacks (metadata por versión)

**M4.1 — Mapeo de IDs a nombres**
- Los eventos crudos referencian unidades/habilidades por ID numérico; necesitas el "datapack" que traduce ID → nombre para cada versión del juego.
- Empieza soportando **solo 1-2 versiones recientes** (no todas las de la historia del juego) — esto es una simplificación consciente y razonable para un proyecto de aprendizaje.
- *Test*: nombres de unidad en tu build order coinciden textualmente con los de Python.

---

## Fase 5 — Robustez y pulido

**M5.1 — Manejo de errores real**
- Replays corruptos, versiones no soportadas, archivos incompletos — que el crate falle con errores claros, no panics.

**M5.2 — Suite de tests de regresión**
- Un corpus de 15-20 replays variados con snapshots esperados (JSON exportado una vez desde Python) que corras en CI o localmente antes de cualquier cambio.

**M5.3 (opcional) — Rendimiento**
- Una vez que la corrección esté validada, medir y comparar velocidad de parsing contra sc2reader en Python con el mismo corpus. Esto es donde Rust debería lucirse, pero solo tiene sentido medirlo *después* de tener corrección.

**M5.4 (opcional) — Documentación y publicación**
- README con ejemplos de uso, quizás publicarlo en crates.io si quieres que sirva también a la comunidad.

---

## Notas sobre alcance y expectativas

- **No te obsesiones con cubrir el 100% de versiones históricas del protocolo.** Sc2reader tiene más de una década de parches acumulados; tu port de aprendizaje puede legítimamente limitarse a versiones recientes (ej. desde Legacy of the Void en adelante) sin perder valor educativo.
- **Valida constantemente, no al final.** El patrón "implementar → comparar contra Python → corregir" en cada milestone es lo que evita que te pierdas 3 semanas parseando algo mal sin saberlo.
- **Cuando este port esté listo, vuelve al proyecto original** (análisis de tus partidas de SC2 con IA) usando tu propio crate en vez de sc2reader — ese es el "premio final" de haber hecho el port.

---

## Orden sugerido de milestones (resumen)

M0.1 → M0.2 → M0.3 → M1.1 → M1.2 → M1.3 → M2.1 → M2.2 → M2.3 → M2.4 → M2.5 → M2.6 → M3.1 → M3.2 → M3.3 → M3.4 → M4.1 → M5.1 → M5.2 → (M5.3, M5.4 opcionales)
