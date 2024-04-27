# Project DeLTA

**De**centralized **L**and **T**ransportation **A**ssistant

## Development

1. Set up a toolbox container.
   - Run, `toolbox create --image quay.io/toolbx-images/debian-toolbox:12`
2. Set up Rust via `rustup`.
   - Optionally, install `rust-analyzer` via `rustup component add rust-analyzer`.
3. Install the following system dependencies via `apt`:
   - `libgtk-4-dev`
   - `libadwaita-1-dev`
   - `libgstreamer1.0-dev`
   - `gstreamer1.0-plugins-good`
4. Use `run` to build and run the project.
