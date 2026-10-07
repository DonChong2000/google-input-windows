# AGENTS.md

Notes for coding agents working on this repo. What the app must do is in
`requirement.md`; this file covers how the code is built and the traps in it.

## What this is

A Windows tray app (`google-input.exe`) that copies the Google Input Tools Chrome
extension (`mclkkofklkfljcocdinagocijmpgbhab`, v102) so it works in every app,
not only the browser. The goal is to copy the extension as closely as is
reasonable. When the code and the extension disagree, the extension wins. Any
difference kept on purpose goes under "Known limits" in `requirement.md`.

## Build and test

- Toolchain: rustup `stable-x86_64-pc-windows-gnu`. This machine has no MSVC.
- `windows-sys` needs a full mingw-w64 (WinLibs) on PATH. rustup's self-contained
  one has no `as.exe`, so `dlltool` fails.

```sh
cargo test               # engine, tools, google parser, config, popup paint
cargo build --release    # target/release/google-input.exe
```

- Only one instance can run (named mutex). Kill the old exe before testing a new
  build.
- CI (`.github/workflows/build.yml`, windows-latest, MSVC) runs the tests and
  builds on every push and PR. Pushing a `v*` tag also creates a GitHub release
  with the exe attached: `git tag v0.1.0 && git push origin v0.1.0`.
- Settings live in `%APPDATA%\GoogleInputTools\settings.json`. Delete the file to
  get the defaults back (廣東話 only).

## Layout

| File | Role |
|---|---|
| `engine.rs` | The extension's composition logic as a pure state machine. `Engine::key(Key) -> Out { swallow, commit, wait }`, plus `view()` for drawing. No Win32 or network code. Most behavior changes belong here, with a test. |
| `tools.rs` | The `TOOLS` registry (41 Google tools + 日本語), built with `zh()`, `yue()`, `cangjie()`, `tr()`, `ctx()` and `rtl()`. Also per-tool punctuation and digit conversion (`convert`), full width, and cangjie radicals. |
| `google.rs` | Builds the `inputtools.google.com/request` URL (pre-context `\|ctx,src`) and parses the response, including `matched_length` and annotations. |
| `config.rs` | `Config` (serde, persisted), `Hotkey`, and the tool commands: next, revert, toggle, select. |
| `main.rs` | Glue: low-level keyboard and mouse hooks, fetch threads, the tray, the menu and message routing. |
| `popup.rs` | Candidate box drawn to match `chext_inputtoolsbin.css` (`ita-ppe-*`). |
| `statusbar.rs` | Draggable 中/En, width and punctuation bar shown for Chinese tools. |
| `prefs.rs` | Options window: tool lists, four hotkey pickers, Shift-tap option. |
| `gdi.rs` | GDI helpers: fonts, text, double buffering, DPI, caret position (GUI thread info, then MSAA). |

## Ground truth

The unpacked extension is not in the repo. To check a behavior, unpack the `.crx`
and read:
- `chext_driver.js`: the request (`sw`, `rw`, `Dw`)
- the `tC.I()` key handler
- `chext_backgroundpage.js`: the commands
- `imeconfigs/*.js`: per-tool settings

Config keys used:
- 3: trailing space
- 4: Enter commits raw
- 5: context
- 6: caret movement
- 11: vertical layout
- 14: page size
- 19: punctuation
- 21: transform (cangjie radicals)
- 22: extra composition characters
- 24/25: page keys
- 30: case sensitivity
- 31: annotations

## Rules that bite

- **Never block the keyboard hook.** Windows silently removes a low-level hook
  that is too slow. The hook only decides swallow or pass, then posts messages:
  - drawing goes through `WM_RENDER`
  - network fetches run on a thread and post `WM_CANDS` back
  - a key typed before the answer arrives is held, the keys after it queue, and
    `TIMER_WAIT` gives up after 2 s
- **Don't feed your own input back in.** Text is sent with `SendInput`
  `KEYEVENTF_UNICODE` and `dwExtraInfo = 0x47494E50`. The hook skips anything
  carrying that tag.
- **No Win32 call with an empty buffer.** `"".encode_utf16().collect::<Vec<_>>()`
  has a dangling pointer, and `DrawTextW` reads it even at length 0. Inside
  WM_PAINT this kills the process with 0xC000041D and no panic message.
  `gdi::draw_text` returns early on empty strings; keep it that way.
- **Popups never take focus** (`WS_EX_NOACTIVATE`).
  - The tray menu needs `force_foreground`, which uses AttachThreadInput.
  - The Activate shortcut opens the menu only after its modifier keys are
    released. Opening it earlier left Shift stuck down in the target app.
- **Alt shortcuts:** inject the 0xE8 mask key so releasing Alt doesn't open the
  target app's menu bar.

## End-to-end check

None is in the repo. The last full check drove Notepad-like typing through a
PowerShell harness: it ran in the foreground, sent real keys, and read back the
committed text. It covered 19 cases: pick, partial match, fast typing,
punctuation, clicks, Ctrl+. / Shift+Space / Shift tap, tool switching and the
options window.

Rules for writing one:
- Make sure the test window has focus before every key, and abort if it doesn't.
  Otherwise the keys land in whatever window the user is in.
- Make the test process DPI aware, or screenshots come out offset.

## Conventions

- Mark deliberate simplifications with `// ponytail:` and name the limit.
- Change `requirement.md` in the same commit as any behavior change. If the
  change is a deliberate difference from the extension, add it to Known limits.
- Don't install tools (npm and similar) outside a scratch directory.
