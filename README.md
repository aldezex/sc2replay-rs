# sc2reader-rs

Port de aprendizaje de [sc2reader](https://github.com/ggtracker/sc2reader) (Python) a Rust, escrito **desde cero** — sin usar crates de parsing MPQ ya existentes — con el objetivo explícito de aprender Rust a través de un proyecto real con alcance bien definido.

## Objetivo del proyecto

Construir un parser de replays de StarCraft II (`.SC2Replay`) funcionalmente equivalente a sc2reader, validando cada paso contra la salida real de la librería Python original como "oráculo" de corrección.

No es un proyecto pensado para superar a sc2reader ni para producción — es un vehículo de aprendizaje de Rust: parsing binario, manejo de errores idiomático, modelado de dominio con `struct`/`enum`, y organización de un crate en módulos.

## Estado actual

🚧 En desarrollo activo. Fase actual: **Fase 1 — Contenedor MPQ**.

### Completado

- [x] **M0.1** — Entorno, fixtures de replays reales, binario de debug (`src/bin/inspect.rs`).
- [x] **M1.1 / M1.2 (parcial)** — Parsing manual y verificado del `MPQUserData` header (signature `MPQ\x1B`) que envuelve todo `.SC2Replay`.
- [x] **Header MPQ real** — Parsing del header MPQ (`MPQ\x1A`), incluyendo detección de formato **V4** (confirmado por `format_version = 3` + `header_size = 0xD0`, consistentes entre sí).
- [x] Localización de la **hash table** y **block table** (posición y número de entradas de cada una — pendiente leer su contenido).
- [x] Manejo de errores propio (`MpqParseError`, vía `thiserror`) en vez de panics, con tests unitarios sobre bytes construidos a mano.

### En progreso

- [ ] **M1.3** — Desencriptación y lectura del contenido de la hash table (algoritmo de cifrado propio de MPQ, con clave fija conocida).

### Pendiente

Ver [`plan-sc2reader-rust.md`](./plan-sc2reader-rust.md) para el plan completo de milestones (Fases 2-5: protocolo de eventos, capa de dominio, datapacks, robustez).

## Estructura del proyecto

```
sc2reader-rs/
├── src/
│   ├── lib.rs          # declara los módulos públicos del crate
│   ├── mpq.rs           # parsing del contenedor MPQ (MPQUserData, MpqHeader)
│   └── bin/
│       └── inspect.rs   # binario de debug: carga un replay y muestra su estructura
├── fixtures/             # replays .SC2Replay reales usados para pruebas manuales
└── plan-sc2reader-rust.md
```

## Decisiones de diseño

- **Sin crates de parsing MPQ.** Se implementa el contenedor MPQ a mano (a diferencia de `s2protocol-rs`, que sí usa librerías existentes) porque el objetivo es aprender, no llegar rápido.
- **`Result<T, MpqParseError>` en vez de panics** en toda la librería (`src/mpq.rs`). Los panics (`.expect()`) se reservan para el binario de debug (`inspect.rs`), donde fallar ruidosamente es aceptable.
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
