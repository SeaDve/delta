# Project DeLTA

**De**centralized **L**and **T**ransportation **A**ssistant

## Voice Commands

| General Commands                  | Description   |
| --------------------------------- | ------------- |
| `call [peer name]`                | Call a peer.  |
| `alert [sos / hazard / yielding]` | Alert a peer. |

| Active Call Commands | Description            |
| -------------------- | ---------------------- |
| `accept`             | Accept incoming call.  |
| `decline`            | Decline incoming call. |
| `cancel`             | Cancel outgoing call.  |
| `end`                | End ongoing call.      |

## Building and Running

1. Set up a toolbox container.
   - Run, `toolbox create --image quay.io/toolbx-images/debian-toolbox:12`
2. Set up Rust via `rustup`.
   - Optionally, install `rust-analyzer` via `rustup component add rust-analyzer`.
3. Install the required dependencies.

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev libshumate-dev libgstreamer1.0-dev gstreamer1.0-plugins-good libspeechd-dev speech-dispatcher cmake clang gpsd gpsd-clients
```

4. Set up text-to-speech (TTS).
   1. Uncomment the required locale from `/etc/locale.gen`.
   2. Install `locales` via `apt` and run `/usr/sbin/locale-gen`.
5. Set up speech-to-text (STT).

```sh
git clone https://github.com/ggerganov/whisper.cpp.git
cd whisper.cpp
./models/download-ggml-model.sh tiny.en
```

6. Use `./run` to build and run the project.
   - `STT=1 LOCATION=14.676007,120.531093 NAME=ABC-123 ./run`
   - `LOCATION=14.676760,120.530814 NAME=IJK-456 ./run`
   - `LOCATION=14.676251,120.532123 NAME=XYZ-789 ./run`

## Syncing code to the Pi

```sh
rsync --filter=':- .gitignore' --exclude \".*/\" -aP ./ $REMOTE_DIR
```
