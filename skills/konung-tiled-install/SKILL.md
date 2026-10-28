---
name: konung-tiled-install
description: >
  Ставит или обновляет расширение Tiled «konung», которое открывает
  оригинальные карты «Князя: Легенды Лесной страны» (Konung.N) прямо в Tiled
  и сохраняет их обратно, и проверяет, что Tiled его подхватил. Использовать,
  когда пользователь хочет открыть или править карту «Князя» в Tiled,
  поставить или обновить расширение, или Tiled не открывает Konung.N.
---

# Расширение Tiled «konung»

Нужен Tiled 1.12. Расширение только читает `GRAPH.RES` и `Objects.res` и пишет `Konung.N`; файлы игры пользователь берёт из своей копии.

1. **Откуда.** Папка `tiled/konung/` в корне плагина: два уровня вверх от папки этого навыка (`<папка навыка>/../../tiled/konung`). Если работа идёт в клоне re-konung, папка та же, в рабочей копии, и вместо шагов 2–3 хватит `just tiled-install`.
2. **Куда.** Папка расширений Tiled, подпапка `konung`:
   - Windows: `%LOCALAPPDATA%\Tiled\extensions\konung`;
   - macOS: `~/Library/Preferences/Tiled/extensions/konung`;
   - Linux: `${XDG_CONFIG_HOME:-~/.config}/tiled/extensions/konung`.

   Старая папка `konung` заменяется целиком. Если в ней есть файл `VERSION` (стояла версия из релиза), сначала сказать пользователю, что она будет заменена.

   PowerShell:

   ```powershell
   $source = '<корень плагина>/tiled/konung'
   $dest = Join-Path $env:LOCALAPPDATA 'Tiled/extensions/konung'
   if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
   New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
   Copy-Item -Recurse $source $dest
   ```

   bash:

   ```bash
   source="<корень плагина>/tiled/konung"
   case "$(uname)" in
       Darwin) dest="$HOME/Library/Preferences/Tiled/extensions/konung" ;;
       *) dest="${XDG_CONFIG_HOME:-$HOME/.config}/tiled/extensions/konung" ;;
   esac
   rm -rf "$dest" && mkdir -p "$(dirname "$dest")" && cp -r "$source" "$dest"
   ```
3. **Проверить копию.** `git diff --no-index --stat <источник> <назначение>`: пустой вывод — файлы совпадают. Без git — сравнить списки файлов и размеры. В назначении должен быть `konung.mjs`.
4. **Проверить, что Tiled подхватил.** Запущенный Tiled перечитывает расширения сам, перезапуск не нужен. Если есть инструменты tiled-mcp (`tiled_status` отвечает), выполнить `run_script`:

   ```js
   return tiled.mapFormats.filter(name => name.startsWith("konung1-")).length
   ```

   Должно быть 50: расширение регистрирует формат «Князь, карта N» на каждый номер карты. 0 — расширение не загрузилось. Без tiled-mcp — попросить пользователя открыть в Tiled консоль («Вид → Панели → Консоль») и посмотреть, нет ли ошибок загрузки `konung.mjs`.
5. **Открыть карту.** `Konung.N` из папки установленной игры, рядом должны лежать `GRAPH.RES` и `Objects.res`: `tiled_open` с путём к файлу или «Файл → Открыть». Первая карта открывается несколько секунд, дальше ресурсы берутся из кэша. `LIGHTS.N` и `Game.N` не открываются с ошибкой размера — так и должно быть. Если игры нет: нужна своя копия (свой диск или покупка в магазине); других источников не предлагать.
6. **Если не работает.** Показать пользователю ошибку из консоли Tiled как есть. Если похоже на баг Tiled или tiled-mcp — сказать, что не так и как воспроизвести, со ссылкой на репозиторий; не обходить молча.

Как карта раскладывается по слоям и что можно править — `docs/k1/tiled.md` в корне плагина.
