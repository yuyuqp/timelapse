import tempfile
import subprocess
from pathlib import Path
from PIL import Image


def run_screencapture(files: list[Path]) -> None:
    cmd = ["screencapture", "-x"]
    cmd.extend([str(file) for file in files])
    subprocess.run(cmd)


def combine_pics(files: list[Path], output: Path) -> None:
    images = [Image.open(str(file)) for file in files]
    widths, heights = zip(*(i.size for i in images))
    total_width = sum(widths)
    max_height = max(heights)
    new_im = Image.new("RGB", (total_width, max_height))
    x_offset = 0
    for im in images:
        new_im.paste(im, (x_offset, 0))
        x_offset += im.size[0]
    new_im.save(str(output))


def shot(output: Path, max_num_monitors=3):
    temp_dir = Path(tempfile.gettempdir())
    temp_files = [temp_dir / f"shot{i}.png" for i in range(max_num_monitors)]
    
    run_screencapture(temp_files)

    actual_files = [file for file in temp_files if file.exists()]
    combine_pics(actual_files, output)
    _ = [file.unlink() for file in actual_files]
