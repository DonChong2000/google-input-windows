# Google Input Tools for Windows (unofficial)

Google Input Tools only works in Chrome. This is a small Windows tray app that
brings the same input method to every app: the same candidates, the same
candidate box and the same keyboard shortcuts.

**Not affiliated with or endorsed by Google.** It calls the same
`inputtools.google.com` endpoint the Chrome extension uses. That endpoint is
undocumented, so it may change or stop working without notice.

## Input tools

42 tools:
- **Chinese:** 拼音, 双拼 (6 schemes), 五笔, 漢語拼音, 倉頡, 倉頡（五代）, 速成,
  廣東話, 粵拼
- **Transliteration:** Amharic, Arabic, Bengali, Belarusian, Bulgarian, Greek,
  Gujarati, Hebrew, Hindi, Kannada, Malayalam, Marathi, Nepali, Odia, Persian,
  Punjabi, Russian, Sanskrit, Serbian, Sinhala, Tamil, Telugu, Thai, Tigrinya,
  Ukrainian, Urdu, Vietnamese
- **Japanese**

See [requirement.md](requirement.md) for the exact behavior and the known
differences from the extension.

## Install

1. Download `google-input.exe` from Releases. Each release is built from its
   tagged source by GitHub Actions (`.github/workflows/build.yml`). Or build it
   yourself: `cargo build --release`.
2. Run it. It sits in the tray.
3. To start it with Windows, put a shortcut in `shell:startup`.

## Use

- Right-click the tray icon to pick input tools, open Options, or exit.
- Left-click the tray icon to turn it on or off.
- Shortcuts, as in the extension (all can be changed in Options):

| Shortcut | Command |
|---|---|
| Alt+Shift+T | Turn the current input tool on/off |
| Alt+Shift+N | Next input tool |
| Alt+Shift+R | Previous input tool |
| (unset) | Open the input tool menu |

- Chinese tools: Shift switches Chinese/English, Shift+Space switches half/full
  width, and Ctrl+. switches punctuation.

## Privacy

While you compose, the letters you type (and up to 20 characters you typed just
before, for context) are sent to Google to get candidates. Nothing is sent while
the tool is off. Settings are stored in
`%APPDATA%\GoogleInputTools\settings.json`.

The app uses a global keyboard hook. Some antivirus software flags that pattern.

## Limits

- Needs internet.
- Can't type into apps running as administrator unless it runs as administrator
  too.
- 注音 (Zhuyin) is not included.

## License

MIT
