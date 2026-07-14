# sc2reader-rs

Port de aprendizaje de [sc2reader](https://github.com/ggtracker/sc2reader) (Python) a Rust, escrito **desde cero** — sin usar crates de parsing MPQ ya existentes — con el objetivo explícito de aprender Rust a través de un proyecto real con alcance bien definido.

## Objetivo del proyecto

Construir un parser de replays de StarCraft II (`.SC2Replay`) funcionalmente equivalente a sc2reader, validando cada paso contra la salida real de la librería Python original como "oráculo" de corrección.

No es un proyecto pensado para superar a sc2reader ni para producción — es un vehículo de aprendizaje de Rust: parsing binario, manejo de errores idiomático, modelado de dominio con `struct`/`enum`, y organización de un crate en módulos.

## Estado actual

🚧 En desarrollo activo. Fase actual: **Fase 1 — Contenedor MPQ**.

### Cambio de arquitectura: extracción de `mpq-parser`

El parsing del contenedor MPQ (que no es específico de StarCraft II — es un formato genérico de Blizzard) se extrajo a su propia librería independiente y publicada: **[mpq-parser](https://crates.io/crates/mpq-parser)** ([repo](https://github.com/aldezex/mpq-parser)).

`sc2reader-rs` ahora depende de `mpq-parser` como una dependencia externa real (vía crates.io), no como código propio. Esto añadió al proyecto un aprendizaje extra no previsto en el plan original: gestión de un crate independiente, versionado semántico, y publicación real en el registro.

### Completado

- [x] **M0.1** — Entorno, fixtures de replays reales, binario de debug (`src/bin/inspect.rs`).
- [x] **M1.1 / M1.2 (parcial)** — Parsing manual y verificado del `MPQUserData` header (signature `MPQ\x1B`) que envuelve todo `.SC2Replay`. *(ahora vive en `mpq-parser`)*
- [x] **Header MPQ real** — Parsing del header MPQ (`MPQ\x1A`), incluyendo detección de formato **V4** (confirmado por `format_version = 3` + `header_size = 0xD0`, consistentes entre sí). *(ahora vive en `mpq-parser`)*
- [x] Localización de la **hash table** y **block table** (posición y número de entradas de cada una — pendiente leer su contenido). *(ahora vive en `mpq-parser`)*
- [x] Manejo de errores propio (`MpqParseError`, vía `thiserror`) en vez de panics, con tests unitarios sobre bytes construidos a mano. *(ahora vive en `mpq-parser`)*
- [x] `mpq-parser` publicado como `v0.1.0` en crates.io.

### En progreso

- [ ] **M1.3** (en `mpq-parser`) — Desencriptación y lectura del contenido de la hash table (algoritmo de cifrado propio de MPQ, con clave fija conocida).

### Pendiente

Ver [`plan-sc2reader-rust.md`](./plan-sc2reader-rust.md) para el plan completo de milestones (Fases 2-5: protocolo de eventos, capa de dominio, datapacks, robustez).

## Estructura del proyecto

```
sc2reader-rs/
├── src/
│   ├── lib.rs          # declara los módulos públicos del crate
│   └── bin/
│       └── inspect.rs   # binario de debug: carga un replay y muestra su estructura,
│                          usando mpq-parser (dependencia externa) para el contenedor MPQ
├── fixtures/             # replays .SC2Replay reales usados para pruebas manuales
└── plan-sc2reader-rust.md
```

El parsing del contenedor MPQ en sí vive en el crate separado [mpq-parser](https://github.com/aldezex/mpq-parser), no en este repo.

## Decisiones de diseño

- **Sin crates de parsing MPQ de terceros.** Se implementa el contenedor MPQ a mano en `mpq-parser` (a diferencia de `s2protocol-rs`, que sí usa librerías existentes) porque el objetivo es aprender, no llegar rápido.
- **Separación en dos crates.** El contenedor MPQ es un formato genérico de Blizzard, no específico de SC2 — se extrajo a `mpq-parser` como librería y proyecto independientes, publicados en crates.io, para no acoplar innecesariamente dos objetivos distintos (formato de contenedor vs. protocolo de replay de un juego concreto).
- **`Result<T, MpqParseError>` en vez de panics** en toda la lógica de parsing (dentro de `mpq-parser`). Los panics (`.expect()`) se reservan para el binario de debug (`inspect.rs`), donde fallar ruidosamente es aceptable.
- **Constantes con nombre para offsets** (`header_offsets::ARCHIVE_SIZE`, etc.) en vez de números mágicos en los rangos de slice, para que el código sea legible sin tener la spec MPQ delante.
- **`thiserror`** para generar `Display`/`Error` sobre `MpqParseError`, tras haber implementado ambos a mano una vez para entender qué hacen.

## Cómo correrlo

```bash
cargo run --bin inspect -- fixtures/tu_replay.SC2Replay
cargo test
```

## Metodología de trabajo

Cada milestone se implementa siguiendo el mismo patrón:
1. Investigar la especificación del formato (fuente: código de sc2reader, spec de `s2protocol`, documentación de la comunidad MPQ).
2. Calcular/verificar los valores esperados **a mano** (hex editor + aritmética little-endian) antes de escribir código.
3. Implementar el parsing en Rust.
4. Comparar el output contra los valores verificados a mano y/o contra `sc2reader.load_replay()` en Python.

No se avanza a un milestone nuevo sin verificación del anterior.

## Recursos usados

- [sc2reader (Python)](https://github.com/ggtracker/sc2reader) — especificación de facto del comportamiento a replicar.
- [Blizzard/s2protocol](https://github.com/Blizzard/s2protocol) — referencia del protocolo de serialización de eventos.
- Documentación de la comunidad sobre el formato MPQ (StormLib / wiki de modding) para el contenedor.
- [mpq-parser](https://github.com/aldezex/mpq-parser) — librería propia (crate hermano) para el parsing del contenedor MPQ.
- [nom-mpq](https://lib.rs/crates/nom-mpq) — parser MPQ usado por `s2protocol`, con enfoque distinto (parser combinators vía `nom`); referencia interesante, no usada como dependencia.
