# Requirements: Google Input Tools for Windows

"Google Input Tools" below means the
Chrome extension `mclkkofklkfljcocdinagocijmpgbhab` (v102). Its behavior is what we
copy.

## Why

Google Input Tools only works inside the browser. The goal is the same input method
(same candidates, same look, same shortcuts) in every Windows app.

**Overall goal: replicate Google Input Tools as closely as is
reasonable.** When this document and the extension disagree, the extension wins.
Any difference we keep on purpose is listed under "Known limits".

## R1. Type anywhere on Windows
- Works in any app that accepts keyboard text (Notepad, Office, Chrome, Electron,
  terminals, and so on). It is not limited to the browser.
- Runs as a tray app. The tray icon shows the active input tool's glyph (粵, 拼,
  倉, and so on) in blue when on and grey when off.
- Only one instance runs at a time.
- It must never take focus from the app you're typing in, and it must never stall
  typing. Drawing and network calls happen outside the keyboard hook. Windows
  silently removes a hook that is too slow.

## R2. Same candidates as Google
Requests are made the way the extension makes them (`chext_driver.js`: `sw`, `rw`,
`Dw`):
- **Request:** a POST with an empty body to
  `https://inputtools.google.com/request?text=…&itc=<tool>&num=<2×page+1>&cp=0&cs=1&ie=utf-8&oe=utf-8&app=chext`.
- **Pre-context:** tools that send it (廣東話, 粵拼, pinyin and shuangpin, hi, ru,
  uk, be, he, th, vi) send `text=|<last ≤20 chars typed>,<letters>`. We only know
  text we typed ourselves; a click or another window clears it.
- **Partial matches** (`matched_length` shorter than the typed letters): the chosen
  part is held, underlined in the box, and the rest is looked up again. Nothing is
  typed into the app until the whole composition is chosen. Backspace un-picks the
  last held part.
- **More candidates:** paging past the loaded list asks again with twice as many.
- **Network faster than typing:** Space or punctuation pressed before the answer is
  a delayed commit. The key is held and the keys after it queue in order. When the
  answer arrives, the first candidate is committed whole (unmatched letters are
  dropped) and then the punctuation; after 2 s the raw letters are used instead. A
  digit pressed before the answer is swallowed.

## R3. The input tools Google offers
The extension offers 42 IMEs. All of them are ported except 注音 (it uses a
separate bopomofo model). 日本語 is added because the API serves it, although the
extension hides it. Names are the extension's native names, in its order:

| Group | Tools |
|---|---|
| Chinese | 拼音, 双拼 ×6 (智能ABC, 微软方案, 小鹤, 拼音加加, 紫光, 自然码), 五笔, 漢語拼音, 倉頡, 倉頡（五代）, 速成, 廣東話, 粵拼 |
| Others | አማርኛ, العربية, বাংলা, Ελληνικά, ગુજરાતી, हिन्दी, עִבְרִית, ಕನ್ನಡ, മലയാളം, मराठी, नेपाली, ଓଡ଼ିଆ, فارسی, ਪੰਜਾਬੀ, Русский, संस्कृतम्, Српски, සිංහල, தமிழ், తెలుగు, ትግርኛ, اردو, Tiếng Việt, Беларуская, Български, Українська, ภาษาไทย |
| Extra | 日本語 |

Per-tool settings come from each tool's `imeconfigs/*.js`:
- **Page size:** 廣東話/粵拼/五笔 show 6; pinyin, shuangpin and cangjie show 5;
  vi shows 8; the rest show 6.
- **Layout:** 廣東話 and 粵拼 are vertical. Pinyin, shuangpin, wubi and cangjie
  are a single horizontal row.
- **Annotations:** grey romanization after each candidate for 廣東話, 粵拼, 倉頡
  and 五笔.
- **Cangjie:** shows 日月金… radicals instead of letters, and `.` `*` are
  wildcard letters.
- **Non-Chinese tools:**
  - A space after every commit, including punctuation and raw commits.
  - Enter commits the highlighted candidate, and letters are case-sensitive.
  - The typed text itself is offered as a candidate at the end of the first page.
  - Extra composition characters per tool:
    - ru/bg: ``' [ ] \ ` ``
    - be/uk: `' [ ] \`
    - hi/kn/mr/ne: digits (they select only after a letter) and `^ ~ |`
    - ar: digits, `` ` _ - ' `` and Space
    - ur: digits
    - ta: `^ _`
    - te: `^ ~ | @`
    - am/el/he/ti: `` ` ``
    - fa: `` ` ' ``
    - sr: `ĆćČčĐđŠšŽž`
- **Numbering:** none for ar/ur; native digits for bn/fa/kn/mr.
- **Punctuation:**
  - am/ti: `.` → `።`, `,` → `፣`
  - ar/ur/fa: `; ?` → `؛ ؟`; ar and ur also `,` → `،`
  - bn/kn/mr/fa/ur: digits become native digits
  - bn/kn/mr/gu/ne: `|` → `।`

Excluded: Google's on-screen keyboards, handwriting and voice. They are not
text-transliteration services.

## R4. Same UI as Google Input Tools
The candidate box follows `chext_inputtoolsbin.css` (`ita-ppe-*`):
- **Box:** white, 1px `#cdcdcd` border, 6px padding, drop shadow, placed below the
  text cursor (flipped above near the screen bottom; the mouse is used if no cursor
  can be found).
- **Top line:** held text plus the letters (Arial 18px), underlined 2px, with a
  2×18 `#54bdf0` caret at the caret position.
- **Candidates:** `1. word (annotation)` items (16px; annotation 14px
  `rgb(169,169,169)`). The highlight is `#f1f1f1`. While the next answer loads,
  the old list stays, greyed (`#777`). Numbers are hidden for a lone candidate.
- **Page buttons:** 22×18, gray border, `#f5f5f5` at 55% opacity (33% when
  disabled). They sit below a vertical list, or to the right of a horizontal one.
- **Mouse:** clicking a candidate commits it, and the page buttons page. Clicking
  anywhere else discards the composition.

**Status bar** (`ita-kd-statusbar`): shown for Chinese tools while they are on.
- A #eee box, bottom-right by default and draggable (its position is remembered).
- Contents: a grip, the tool glyph, then three buttons: 中/En (Chinese/English),
  ☽/● (half/full width) and °,/·, (Chinese/English punctuation). Clicking a button
  toggles it.
- The tray menu can hide or show it.

## R5. Keys (Chinese tools, from `tC.I()`)
State bits are Chinese mode, full width and Chinese punctuation. The default is
Chinese mode with Chinese punctuation, half width; the setting persists.

| Key | While composing | Idle |
|---|---|---|
| a–z, `'` (and cangjie `.` `*`) | add a letter at the caret | start composing (`'` gives 「」 when Chinese punctuation is on) |
| Uppercase first letter (Shift or Caps) | – | raw mode: no lookup, Space/Enter commit it as typed |
| Space | commit the highlighted candidate (no space added) | passes |
| 1–9 | pick on the current page (beyond the page: ignored) | passes |
| Enter | commit the letters as typed (cangjie: radicals) | passes |
| Esc | discard | passes |
| Backspace | un-pick the held part, else delete before the caret | passes |
| ← → Home End | move the caret inside the composition; the lookup uses the letters before it | passes |
| ↑ ↓ | move the highlight; past the end, load more or wrap | passes |
| PgUp PgDn, `-` `,` / `=` `.` | previous / next page (the punctuation keys only once candidates are shown) | Chinese punctuation |
| Other punctuation | commit the highlighted candidate, then the converted punctuation | converted: `，。、；：？！（）【】《》｛｝～￥……——`, quotes alternate 「」『』 (Traditional) or ‘’“” |
| Tab, Delete | swallowed | pass |
| Ctrl/Alt/Win + key | discard the composition; the key passes | passes |
| Shift tap | commit the letters, switch Chinese/English | switch Chinese/English |
| Shift+Space | commit the letters only | switch half/full width |
| Ctrl+. | discard, switch punctuation | switch punctuation |

- Full width (when on) converts letters (English mode only), digits and symbols to
  their full-width forms.
- Other tools: same keys without the Chinese state, caret movement or punctuation
  table. ← → page (mirrored for RTL), and Home/End highlight the first/last
  candidate.
- Shift+Backspace/Tab/Delete/arrows/PgUp/PgDn are swallowed while composing.
- Switching to another window or clicking elsewhere drops the composition.

## R6. Selectable keyboard shortcuts
The same four commands as the extension, each settable in the options window (a
shortcut-picker control) or cleared:

| Command | Default (same as the extension) | Behavior (`chext_backgroundpage.js`) |
|---|---|---|
| Activate input tool | none | Open the input-tool menu at the text cursor (like clicking the extension button) |
| Select next input tool | Alt+Shift+N | Go to the next of my input tools. After the last one, turn off. From off, start again at the first |
| Revert last input tool | Alt+Shift+R | Swap back to the previous input tool and on/off state |
| Toggle current input tool | Alt+Shift+T | Turn the current input tool on or off |

The Shift-tap Chinese/English switch can be turned off in options. Settings persist
in `%APPDATA%\GoogleInputTools\settings.json`.

## R7. Menu and options (popup.html, options.html)
- **Tray left click:** toggles on/off.
- **Tray right click (and the Activate shortcut):**
  - my input tools (the active one checked; click it to turn off; click another
    to switch, which also forces Chinese mode)
  - Turn off
  - Show/Hide Status Bar
  - Options…
  - Keyboard Shortcut Settings…
  - Exit
- **Options window:**
  - a filter box
  - "All input tools" and "Selected input tools" lists, with Add, Remove, Up and
    Down (the order is the Select-next order)
  - the four shortcuts
  - the Shift-tap option
- Removing the current tool turns the input method off.

## R8. Implementation
- Rust, one `.exe`, no installer, no admin rights needed.
- `engine.rs` holds the extension's composition logic as a pure, unit-tested state
  machine.
- PIME was considered and rejected: 2.0 is beta-only, it needs admin to install,
  its candidate window can't be restyled to match Google, and its single-threaded
  backend would make HTTP calls stall typing in every app.

## Known limits (accepted)
- It needs internet.
- It can't type into apps running as administrator unless it also runs as
  administrator (Windows blocks this).
- 注音 (Zhuyin) is not ported.
- The Indic danda (`.` → `।` after a long word) is not ported, nor are the local
  fallback candidates (config key 16) that non-Chinese tools append when the server
  returns few.
- `^` and `_` in Chinese punctuation mode give `……` / `——`. The extension's code
  throws on those two keys (reproduced in Node), so there they probably type a
  plain `^` / `_`.
- Arabic: Space is a composition letter only while composing; an idle Space types
  a space.
- RTL tools (ar, fa, he, ur) render text correctly, but the box layout is not
  mirrored.
- Pre-context only knows what this app typed, not text already in the document.
- In raw mode (uppercase first letter), Space commits the word plus a space. The
  extension appears to eat that space (seen in code only), which would make typing
  English sentences in Chinese mode painful.
- After 2 s without an answer, a held key commits the raw letters. The extension
  drops it instead.
- Status-bar icons are drawn as text glyphs approximating the extension's sprite.
- It doesn't start with Windows by itself; add a shortcut to `shell:startup`.
