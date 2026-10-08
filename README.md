# Zap Faster

A community-friendly fork of [ZapFast](https://github.com/crmne/zapfast), maintained by a daily user to ship fixes and improvements faster.

Zap Faster is a small native WhatsApp client for Windows, Linux and macOS, built with Rust, [egui](https://github.com/emilk/egui), [whatsapp-rust](https://github.com/oxidezap/whatsapp-rust) and [fastframe](https://github.com/crmne/fastframe). It links to your phone as a companion device. No browser engine, telemetry, hosted backend or Zap Faster account is required.

The original project and its contributors made this possible. This fork preserves the MIT license, copyright notices and contributor credit. It has its own maintenance and release schedule. Contributions and fixes from either project are welcome.

## Download and run

Download installers and portable builds from **[Zap Faster releases](https://github.com/eternalwaitt/zap-faster/releases)**. Upstream ZapFast installers, its Homebrew tap, AUR packages and website downloads install the original project.

The first fork release is being verified. Until a release is published, build from source with the toolchain in `rust-toolchain.toml`:

```sh
git clone https://github.com/eternalwaitt/zap-faster.git
cd zap-faster
cargo run --locked --release
```

Linux builds need the development packages for ALSA, OpenGL, Wayland and xkbcommon, plus CMake, Clang and Perl. Windows builds need Visual Studio C++ Build Tools, CMake, LLVM/libclang and Perl. macOS builds need Xcode Command Line Tools and CMake. See [the build and packaging guide](PACKAGING.md).

## What this fork adds

- More reliable text selection, screenshot pasting and scrolling; Windows attachment dragging.
- Media albums, attachment drafts tied to their chat, crop/rotate before sending, and image navigation.
- Private group replies, multi-recipient forwarding, batch deletion and safer failed edits.
- Unsent drafts at the top of the chat list and an option to keep the list position after sending.
- Audio files distinct from voice notes, AAC video sound, playback speeds and motion photos.
- On-demand local Whisper voice transcripts, with optional automatic transcription (off by default).
- Chat links as unsent drafts, unsaved phone-number chats, location sharing and phone-number actions.
- Screen privacy, read-receipt privacy, attachment storage usage and notification throttling.
- Group descriptions and disappearing-message timers, keyboard shortcuts and mouse-side navigation.

The [upstream intake ledger](docs/upstream-intake.md) records individual PRs, original authors, imported revisions and work that still needs adaptation. A listing there is not a promise that an unmerged feature is available.

## Existing ZapFast users

Quit ZapFast before starting Zap Faster. The fork intentionally retains ZapFast's existing settings, session, encrypted archive, OS keyring identity and single-instance lock. It uses the existing `zapfast` data directories; installing both does not create independent accounts. Back up your data before switching, and use one application at a time. The fork does not read your archive during builds or development.

Zap Faster checks its own GitHub releases for updates and verifies them with its own Ed25519 signing key. It does not consume upstream ZapFast updates. Update signatures are separate from Windows Authenticode and Apple notarization; download pages state any platform signing limitations.

The [settings and files guide](docs/_reference/settings-and-files.md) describes local storage, encryption and network behavior. GIF search contacts Giphy only when used and configured; preview links and location map links may contact the relevant website when opened. Whisper downloads its 1.5 GB multilingual model from Hugging Face when first requested, verifies its pinned SHA-256 digest, and transcribes on your CPU. Transcripts are stored in the encrypted account archive; model files are in the account cache. No feature uploads message content to a transcription or analytics service.

## Contribute

Open [issues](https://github.com/eternalwaitt/zap-faster/issues) and [pull requests](https://github.com/eternalwaitt/zap-faster/pulls) here. See [CONTRIBUTING.md](CONTRIBUTING.md) for the checks and privacy requirements. For an upstream issue, link it and describe whether you can reproduce it in Zap Faster. Imported fixes retain the source link and author credit.

We continue to review upstream changes and can contribute compatible fixes back. There is no deadline for ending this fork; its purpose is a reliable daily client and a useful development loop.

```sh
cargo run --locked --features demo -- --demo
```

The demo uses synthetic conversations offline. [DEMO.md](DEMO.md) documents its screens and layout checks.

## Disclaimer and license

This is an unofficial client, unaffiliated with WhatsApp or Meta. Using unofficial clients may violate WhatsApp's terms and risk account suspension.

MIT; see [LICENSE](LICENSE) and [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md). Original ZapFast copyright and attribution are retained. Fonts, icons, wallpaper and notification sounds keep their respective licenses.
