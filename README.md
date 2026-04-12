# Timelapse

Cross-platform screenshot collection and timelapse rendering.

## Project Structure

- src layout package: src/timelapse
- compatibility launcher: main.py
- packaging metadata: pyproject.toml

## Requirements

- Python 3.11+
- ffmpeg available in PATH

## Install

Create a virtual environment and install in editable mode:

pip install -e .

## Usage

Use either the installed console script or module entrypoint:

- timelapse --help
- python -m timelapse --help

Compatibility launcher also works from repo root:

- python main.py --help

### Collect

python -m timelapse collect -d C:/path/to/shots -r 6 -w 10

Options:

- --continue to append frames in an existing directory
- --screen-mode all|current
- --temp-pics to collect into a temporary directory

### Render

python -m timelapse render -d C:/path/to/shots -o C:/path/to/videos -f 15

Rendering auto-detects:

- frame-number width from filenames
- sequence start number for resumed sessions

### Clean

python -m timelapse clean -d C:/path/to/shots -o C:/path/to/videos

Variants:

- --pics-only cleans only .png screenshots
- --videos-only cleans only .mp4 outputs

