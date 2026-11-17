# kn-movement

Детерминированное движение без K1, файловой системы и Bevy.

- `Passage` передаёт проходимость, `Pace` — длительность шага в тиках.
- `Occupancy` хранит стоянки и резервы; `route` ищет маршрут, `walk` выполняет шаги.
- `World` принимает приказы, разрешает спор за клетки и выдаёт события через `tick_step`.
- `StepDurations` хранит готовые длительности; `Rng` обеспечивает повторяемость.
- `metric` задаёт стоимость пути; Cell, Direction, Grid и проекция импортируются напрямую из kn-isometric.

Публичный API — `src/lib.rs`. Проверки: `cargo test --manifest-path crates/Cargo.toml -p kn-movement`.
Замеры `movement_cost` и `removal_cost` запускаются явно в release.
Решения и условия замеров: [перемещение](../../../docs/development/rust/movement.md).
