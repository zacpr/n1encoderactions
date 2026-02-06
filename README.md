# N1 Encoder Actions Plugin

An OpenDeck plugin that provides configurable multi-action support for the Ajazz N1 encoder dial.

## Features

- **Encoder Rotate Actions** - Configure what happens when you rotate the dial:
  | Mode | Clockwise (+1) | Counter-Clockwise (-1) |
  |------|----------------|------------------------|
  | **Volume** | Volume Up `5%+` | Volume Down `5%-` |
  | **Media Track** | Next Track | Previous Track |
  | **Media Seek** | Seek Forward `+5s` | Seek Backward `-5s` |
  | **Scroll** | Scroll Down | Scroll Up |
  | **Brightness** | Brightness Up `10%+` | Brightness Down `10%-` |
  | **Custom** | Your command | Your command |

- **Encoder Press Actions** - Trigger multi-actions when the dial is pressed/released

- **Multi-Action Support** - Add multiple encoder actions with different modes

## Input Mapping

Based on N1 HID protocol discovery:
| Input | Value | Description | Mapped To |
|-------|-------|-------------|-----------|
| `INPUT 50` | `0x32` | Rotate Counter-Clockwise (-1) | Encoder 2 twist -1 |
| `INPUT 51` | `0x33` | Rotate Clockwise (+1) | Encoder 2 twist +1 |
| `INPUT 35` | `0x23` | Dial Press | Encoder 2 press/release |

## Dependencies

Install the required tools for the modes you want to use:

```bash
# Volume mode (alsa-utils)
sudo apt install alsa-utils

# Media control mode (playerctl)
sudo apt install playerctl

# Scroll mode (xdotool)
sudo apt install xdotool

# Brightness mode (brightnessctl)
sudo apt install brightnessctl
```

## Building

```bash
cd n1encoderactions
cargo build --release
```

The binary will be at `target/release/n1encoderactions`.

## Installation

1. Build the plugin
2. Create a plugin folder with these files:
   ```
   net.ashurtech.plugins.n1encoderactions.sdPlugin/
   ├── manifest.json
   ├── n1encoderactions (or .exe on Windows)
   └── inspector.html (optional - for configuration UI)
   ```
3. In OpenDeck: Plugins → Install from file

## Technical Details

This plugin uses the same `mirajazz` crate as the main `opendeck-ajazz-n1` plugin for device communication:
- Watches for N1 device via HID (VID: 0x0300, PID: 0x3007)
- Uses protocol version 3 (software mode)
- Maps dial inputs to encoder index 2 (0-indexed)
- Forwards button/encoder events to OpenDeck for multi-action support

## Plugin Architecture

```
┌─────────────────────────────────────┐
│  OpenDeck Application               │
├─────────────────────────────────────┤
│  N1 Encoder Actions Plugin          │
│  ┌───────────────────────────────┐  │
│  │ Global Handler                │  │
│  │ - Plugin lifecycle            │  │
│  └───────────────────────────────┘  │
│  ┌───────────────────────────────┐  │
│  │ Device Watcher                │  │
│  │ - HID device discovery        │  │
│  │ - Hot-plug support            │  │
│  └───────────────────────────────┘  │
│  ┌───────────────────────────────┐  │
│  │ Input Processor               │  │
│  │ - INPUT50 → EncoderTwist(-1)  │  │
│  │ - INPUT51 → EncoderTwist(+1)  │  │
│  │ - INPUT35 → Encoder press     │  │
│  └───────────────────────────────┘  │
│  ┌───────────────────────────────┐  │
│  │ Action Executor               │  │
│  │ - Volume/Media/Scroll/etc     │  │
│  └───────────────────────────────┘  │
├─────────────────────────────────────┤
│  mirajazz crate (HID protocol)      │
├─────────────────────────────────────┤
│  Ajazz N1 Hardware                  │
└─────────────────────────────────────┘
```
