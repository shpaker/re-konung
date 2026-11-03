# kn-render

Отображение сцены игр серии «Князь» на [Bevy](https://bevyengine.org): тонкий
слой над общей моделью [`kn-game`](../../../game/kn-game). Игровой логики и знания
форматов здесь нет.

## Модули

- `textures` — `TextureCache`: RGBA из фасада → `Image`, один ассет на ключ
  (`kn_game::TextureId`). Уникальных картинок на карте в разы меньше кусков,
  поэтому спрайты делят `Handle<Image>`. `load` отдаёт текст причин по ключам,
  картинка которых не собралась, — потребитель ругается, а не молча теряет
  спрайт.
- `scene` — куски сцены → спрайты. Пиксельные координаты карты (y вниз, угол
  слева вверху) переводятся в мир Bevy (y вверх, позиция по центру); порядок
  наложения передаётся через z, потому что прозрачные спрайты 2D сортируются
  по возрастанию z (`bevy_sprite_render`: `sort_key = FloatOrd(transform.z)`).
  Спрайты — потомки корня `MapRoot`, которому обязательны `Transform` и
  `Visibility`: без них потомки не получают `GlobalTransform` и не рисуются.
- `camera` — `MapCameraPlugin`: camera spawn, the discrete zoom ladder
  (`ZOOM_LEVELS`, `scale = 1 / zoom` keeps whole levels pixel-perfect), map
  bounds, the pending frame (`frame_scene` aims the camera at the scene
  content and picks the fitting step, `fitting_zoom`) and the bounds clamp.
  No input handling: the input mechanisms live in
  [`kn-input`](../input), and each app binds them to the camera in a system
  registered in the `CameraInput` set; `CameraApply` (the write-out:
  `apply_pending_frame` then `apply_zoom`) is ordered after `CameraInput`
  inside this crate.

Также `GameResource` — обёртка `kn_game::Game` в ресурс Bevy.

Bevy используется с `ImagePlugin::default_nearest()`: графика пиксельная,
сглаживание не нужно.

## Проверки

```bash
just cargo-test   # координаты, z-порядок, ступени масштаба, вписывание в окно
just cargo-lint
```

Окно и GPU для тестов не нужны; визуальная сверка карты — вручную, рядом с
PNG из `uv run konung k1 map` (README репозитория).
