# audioctl

audioctl управляет громкостью, mute и устройством по умолчанию для вывода и микрофонов в Windows. Одна иконка в трее показывает текущее устройство вывода. Тот же язык команд работает из командной строки, окна, горячих клавиш, меню трея, HTTP, WebSocket и MQTT.

- Иконка трея меняется вместе с устройством вывода: глиф или свой файл, перечёркивание при mute и, по желанию, цифра громкости. Колесо меняет громкость, средняя кнопка переключает mute, правый клик открывает список устройств. Вторая иконка для микрофона включается отдельно.
- Громкость и mute читаются у каждого устройства. Смена в микшере Windows сразу видна в трее, окне и MQTT. Подключение и отключение устройства тоже приходит от Windows, без опроса.
- Устройство по умолчанию ставится для обычного звука и, по настройке, для звонков.
- REST и WebSocket на `127.0.0.1:7686`, по желанию с токеном.
- MQTT с Home Assistant discovery или интеграция HACS из `custom_components/audioctl`.

## Сборка и запуск

Нужны Windows 10 или 11, [Rust](https://rustup.rs) (MSVC), Node.js 20+ и WebView2. В Windows 11 WebView2 уже установлен.

```powershell
npm install
npm run dev      # разработка, интерфейс с горячей перезагрузкой
npm run build    # релиз и установщики NSIS/MSI в target/release/bundle
```

Конфиг лежит в `%APPDATA%\audioctl\config.toml`, свои иконки в `%APPDATA%\audioctl\icons\`, лог в `%APPDATA%\audioctl\audioctl.log`. Конфиг можно править на экране Settings.

Повторный запуск не создаёт вторую копию. Если в аргументах есть команда, она уходит в уже запущенное приложение. Если команды нет, открывается окно. Автозапуск стартует скрытым, с флагом `--hidden`.

## Скачать

Сборки Windows появляются на [странице релизов](https://github.com/choopachuck/audioctl/releases). `audioctl-<версия>-x64-setup.exe` — установщик, `audioctl-<версия>-x64.exe` — сама программа.

Релиз собирается по тегу `v*`:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

## Язык команд

Одна строка работает в CLI, `POST /api/command`, WebSocket, MQTT, горячих клавишах и пунктах меню трея.

Перед командой можно указать устройство: имя, алиас, ключ, номер или подпись из списка Home Assistant. Без префикса команда относится к текущему выводу. `out` и `in` — текущие устройства вывода и ввода. Имена с пробелами берутся в кавычки. После `o` и `i` следующие `v`, `m`, `u`, `t`, `d` и `g` относятся уже к выбранному устройству.

| Команда | Действие |
| --- | --- |
| `v 30`, `v +5`, `v - 5`, `v+10`, `v-5` | громкость, абсолютная или относительная, 0–100 |
| `m`, `u`, `t` | выключить звук, включить, переключить |
| `d` | сделать выбранное устройство основным в своём направлении |
| `o next`, `o prev`, `o Speakers` | устройство вывода. `previous` — то же, что `prev` |
| `i next`, `i Mic` | микрофон |
| `g` | вернуть громкость. В CLI это код выхода |
| `;` `,` | пауза 100 мс и 1 с. Одна пауза не длиннее 10 с |
| `#` | комментарий до конца строки |
| `! notepad.exe` | запустить программу через `cmd /C` |
| `q`, `z` | выход и перезапуск audioctl |

`next` и `prev` перебирают устройства своего направления, которые не скрыты и участвуют в цикле. Если таких два, «следующее» и «предыдущее» указывают на одно и то же другое устройство.

Устройство ищется в таком порядке: точный id, ключ, алиас, имя, подпись с индексом для одинаковых имён, затем номер с 1. Регистр не важен. Номер идёт по списку: сначала вывод, потом микрофоны, внутри группы по имени.

```powershell
audioctl.exe "USB Headset" v 30     # команда уходит в уже запущенную копию
audioctl.exe o next
audioctl.exe g; echo $LASTEXITCODE  # громкость текущего вывода
```

Если API выключен или до запущенной копии не достучаться, эта копия сама стартует и выполняет команду, когда устройства уже известны. `--hidden`, `--minimized` и `/hidden` не входят в текст команды.

Ответ команды — JSON: `messages`, `value` (только у `g`), `quit`, `restart`. Сообщения печатаются в консоль, `value` становится кодом выхода. Без `g` код выхода 0.

## Трей и окно

Иконка показывает текущий вывод: его глиф или файл, красное перечёркивание при mute и, если включено, маленькую цифру громкости. Подсказка содержит имя и громкость. Вторая такая же иконка показывает микрофон; по умолчанию она выключена.

Колесо меняет громкость на шаг из настроек трея. Средняя кнопка переключает mute. Левая открывает окно над треем. Правая открывает меню: устройства вывода с отметкой текущего, подменю микрофонов, свои пункты, Open, Rescan и Quit.

Окно прячется, когда теряет фокус. Это отключается в настройках. Escape тоже прячет окно. Закрытие окна не завершает программу.

В окне две секции, Output и Microphones. В строке устройства иконка, имя, ползунок, mute, выбор «по умолчанию» и индикатор уровня. У устройства задаются алиас, скрытие, участие в `next`/`prev` и иконка. Скрытое устройство остаётся в окне и в API, но пропадает из меню трея, цикла и MQTT.

Экран Console выполняет ту же строку команд и показывает ответы. Settings содержит General, Tray, Hotkeys, API, MQTT и свои пункты меню.

## Иконки

Глифы рисуются шрифтом Segoe Fluent Icons в Windows 11 и Segoe MDL2 Assets в Windows 10. Имя глифа: `speakers`, `headphones`, `headset`, `microphone`, `monitor`, `tv`, `bluetooth`, `webcam`, `gamepad`, `network`, `car`. Цвет задаётся рядом, например `#64D2FF`. Для вывода цвет по умолчанию `#64D2FF`, для микрофона `#30D158`. Тип корпуса, который сообщает Windows, выбирает глиф сам: наушники, гарнитура, HDMI, микрофон и остальные.

Свой файл — PNG, JPEG, GIF, BMP или ICO. При выборе он копируется в `%APPDATA%\audioctl\icons\`. В конфиге путь относительный, `icons/имя.png`. Файл не перекрашивается; перечёркивание и цифра рисуются поверх.

`GET /api/devices/{ref}/icon.png?size=72&badge=1` отдаёт PNG от 16 до 256 пикселей. `badge=1`, `true` или `yes` добавляет цифру громкости.

## Горячие клавиши

Сочетание регистрируется глобально и выполняет команду. Поле в Settings записывает нажатые клавиши. Если сочетание уже занято, предупреждение появляется в окне.

Модификаторы: `Ctrl`, `Alt`, `Shift`, `Win`. Клавиша — буква, цифра, `F1`–`F24`, стрелки, `Space`, `Tab`, `Esc`, `Home`, `End`, `PageUp`, `PageDown`, `Insert`, `Delete`, `Plus`, `Minus`, `Comma`, `Period`, а также `VolumeUp`, `VolumeDown`, `VolumeMute`, `MediaNext`, `MediaPrev`, `MediaPlay`.

```toml
[[hotkey]]
keys = "Ctrl+Alt+F1"
command = "o next"
```

## Конфиг

Файл пишется атомарно. Пустой файл даёт значения по умолчанию.

```toml
[general]
start_with_windows = false
set_communications_default = true   # вместе с обычным устройством менять устройство для звонков
volume_step = 5                      # шаг ползунка в окне

[api]
enabled = true
bind = "127.0.0.1:7686"
# token = "..."

[mqtt]
enabled = false
host = "homeassistant.local"         # также host:port, mqtt://, mqtts://, ssl://, tcp://
port = 1883
discovery_prefix = "homeassistant"
base_topic = "audioctl"
# node_id = "desktop"                # иначе имя компьютера
discovery = true                     # выключить, если установлена интеграция HACS

[tray]
enabled = true
wheel_step = 5
show_input_icon = false
show_volume_badge = false
hide_window_on_blur = true

[[device]]
id = "{0.0.0.00000000}.{guid}"
alias = "Desk"
hidden = false
in_cycle = true
icon = { glyph = "headphones", color = "#64D2FF" }
# icon = { file = "icons/desk.png" }

[[menu]]
label = "Quiet"
command = "out v 10"
```

`0.0.0.0:7686` слушает не только этот компьютер. В этом случае задайте токен. Имя узла MQTT — `node_id` или `COMPUTERNAME`, приведённое к ключу: строчные буквы и цифры, остальное схлопывается в `_`.

## HTTP и WebSocket

По умолчанию `http://127.0.0.1:7686`. Если токен задан, нужен заголовок `Authorization: Bearer <token>` или `?token=` для WebSocket. `{ref}` — ключ, id, номер, имя, алиас или `out`/`in`.

| Метод | Путь | Тело |
| --- | --- | --- |
| GET | `/api/health` | |
| GET | `/api/devices?flow=render` | `flow`: `render`, `output`, `capture` или `input` |
| GET, PATCH | `/api/devices/{ref}` | `{"volume": "+5", "muted": true, "default": true}` |
| GET | `/api/devices/{ref}/icon.png?size=72&badge=1` | PNG |
| GET | `/api/default` | `{"output": {...}, "input": {...}}` |
| PUT | `/api/default/output` | `{"device": "Speakers"}`, `"next"` или `"prev"` |
| PUT | `/api/default/input` | то же для микрофона |
| POST | `/api/command` | `{"command": "o next"}` |
| POST | `/api/rescan` | заново прочитать устройства |
| GET | `/api/ws` | WebSocket |

`volume` в PATCH — число 0–100 или строка `"+5"`, `"-5"`, `"50"`. `default: false` ничего не делает. Лишние поля в теле отвергаются.

Устройство в ответе содержит `id`, `key`, `index`, `name`, `alias`, `display_name`, `flow`, `form_factor`, `adapter`, `volume`, `muted`, `is_default`, `is_default_communications`, `hidden`, `in_cycle`, `icon_glyph`, `icon_char`, `icon_color`, `icon_file`.

WebSocket сначала присылает `{"type":"devices_changed","devices":[…]}`, затем `device_updated`, `levels`, `notification`, `config_changed`, `quit` и `restart`. `levels` — это `{"id","peak"}`, пик от 0 до 1. Если клиент отстал, приходит новый полный список. Клиент шлёт:

```json
{"type": "command", "command": "out t", "id": 1}
{"type": "set", "device": "speakers", "volume": 20, "muted": false, "id": 2}
{"type": "rescan", "id": 3}
```

Ответ: `{"type":"result","id":1,"ok":true,"result":{…}}`. При ошибке `ok` равен `false` и есть `error`.

Без MQTT ту же команду можно вызвать из Home Assistant напрямую:

```yaml
rest_command:
  audio_quiet:
    url: http://127.0.0.1:7686/api/command
    method: POST
    content_type: application/json
    payload: '{"command": "out v 15"}'
```

## MQTT и Home Assistant

Базовая тема — `audioctl/<узел>`, например `audioctl/desktop`. Адрес брокера — `192.168.1.10`, `192.168.1.10:1883`, `mqtt://…` или `mqtts://…`. Порт в адресе важнее поля порта. `mqtts://` и `ssl://` включают TLS. Проверка соединения в Settings открывает короткое подключение и закрывает его.

Нужен один источник сущностей. Discovery и интеграция HACS создают одни и те же ползунки.

### Интеграция HACS

В HACS добавьте репозиторий как Integration и установите audioctl. В Home Assistant сначала должен быть настроен MQTT. Затем добавьте интеграцию audioctl: базовая тема по умолчанию `audioctl`. Идентификатор узла оставьте пустым, чтобы видеть все компьютеры, или укажите узел из Settings.

В audioctl включите MQTT и выключите Publish Home Assistant MQTT discovery. Контракт тем описан в [docs/hacs-integration.md](docs/hacs-integration.md).

На компьютере появляются выбор вывода, выбор микрофона, поле команды и кнопка Rescan. На каждом устройстве — громкость 0–100, mute и кнопка Make default. Скрытые устройства не публикуются. Пустое retained-сообщение `info` убирает устройство, когда оно отключилось.

### MQTT discovery

Оставьте Publish Home Assistant MQTT discovery включённым и префикс `homeassistant`. Сущности появляются без интеграции: `select` Output device и Input device, `text` Command, `button` Rescan, на каждом устройстве `number` Volume, `switch` Mute, `button` Make default и диагностический `binary_sensor` Default. Выключение discovery публикует пустые retained-сообщения и убирает эти конфиги.

| Тема | Кто пишет | Содержимое |
| --- | --- | --- |
| `<base>/status` | audioctl, retained, LWT | `online` или `offline` |
| `<base>/info` | audioctl, retained | имя, версия, текущие вывод и микрофон, списки вариантов |
| `<base>/output/state`, `<base>/input/state` | audioctl | текущая подпись |
| `<base>/output/set`, `<base>/input/set` | Home Assistant | имя, ключ, `next` или `prev` |
| `<base>/<key>/info` | audioctl, retained | описание, громкость, mute, default. Пустое сообщение удаляет устройство |
| `<base>/<key>/state` | audioctl, retained | `volume`, `muted`, `default`, `default_communications`, `name`, `flow` |
| `<base>/<key>/volume/set` | Home Assistant | `0`–`100`, `+5` или `-5` |
| `<base>/<key>/mute/set` | Home Assistant | `ON` или `OFF` |
| `<base>/<key>/default/set` | Home Assistant | любое сообщение делает устройство основным |
| `<base>/command` | Home Assistant | строка команды |
| `<base>/rescan` | Home Assistant | любое сообщение |

Интеграция HACS подписана на `<base>/+/status`, `<base>/+/info` и `<base>/+/+/info`. Громкость она видит, потому что audioctl заново публикует `info` при изменении.

## Структура

```text
crates/audioctl-core      движок, Core Audio, язык команд, конфиг
crates/audioctl-server    REST/WebSocket и MQTT
src-tauri                 окно, трей, горячие клавиши, передача команды из CLI
ui                        интерфейс на Svelte 5
custom_components/audioctl   интеграция Home Assistant для HACS
```

## Тесты

```powershell
cargo test
pytest
npm run check
```

## Лицензия

MIT
