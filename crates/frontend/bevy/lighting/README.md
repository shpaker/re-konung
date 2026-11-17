# kn-lighting

Общий контракт освещаемых поверхностей Bevy и текущая классическая реализация.
Зависимости — только Bevy; игровых форматов и типов сцены здесь нет.

- `LightingSurface` помечает участвующие поверхности; `LightingSet::Apply` задаёт момент применения света.
- `classic` хранит материал, шейдер, входные параметры, мелкую сетку и непрозрачный пул материалов.
- `LightingMaterials::attach` добавляет классические данные и материал при создании сущности; выбор handle скрыт внутри реализации.
- `install_classic_lighting` устанавливает пул до создания спрайтов; `KonungLightingPlugin` регистрирует конвейер.
- Для проверок без GPU `add_classic_surface_system` регистрирует выбор handle без загрузки шейдера.

Проверки: `cargo test --manifest-path crates/Cargo.toml -p kn-lighting`.
GPU-пример: `cargo run --manifest-path crates/Cargo.toml -p kn-lighting --example lighting_probe -- out/lighting-probe.png`.
Он использует синтетические данные; изображение проверяется `tools/check_lighting_probe.py`.
Подробности: [рендеринг](../../../../docs/development/rust/rendering.md).
