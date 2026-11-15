# kn-game

Общий фасад, модель сцены, движок перемещения и симуляция без Bevy.

- Game::open/maps/heroes/scene/textures/check представляет K1 через общие типы.
- Scene/Piece/TextureId/Animation/PieceGroup несут данные отображения без типов фронтенда.
- movement владеет Grid, Cell, Direction, Occupancy, find_route, Walker и World; Passage/Pace передают специфику игры.
- Simulation вычисляет решения K1 и применяет приказы к общему миру, включая задержку повторов отказавших приказов.
- Движок перемещения не выбирает цель; восстановленное поведение вычисляется в k1-game.

Подробности: [тематическая документация](../../../docs/development/rust/README.md).
Проверки: `just cargo-test`, `just cargo-lint`. Чистая логика не требует окна/GPU;
проверки на игровых данных используют локальный original/ и пропускаются без него.
