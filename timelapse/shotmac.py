import tempfile
from contextlib import contextmanager
from pathlib import Path

import anyio
import anyio.to_thread
from PIL import Image


async def run_screencapture(files: list[Path]) -> None:
    cmd = ["screencapture", "-x"]
    cmd.extend([str(file) for file in files])
    await anyio.run_process(cmd)


async def combine_pics(files: list[Path]) -> Image.Image:
    images = await anyio.to_thread.run_sync(
        lambda: [Image.open(str(file)) for file in files]
    )
    widths, heights = zip(*(i.size for i in images))
    total_width = sum(widths)
    max_height = max(heights)
    new_im = Image.new("RGB", (total_width, max_height))
    x_offset = 0
    for im in images:
        new_im.paste(im, (x_offset, 0))
        x_offset += im.size[0]
    return new_im


@contextmanager
def temp_files(n: int):
    temp_files = []
    try:
        temp_dir = Path(tempfile.gettempdir())
        temp_files = [temp_dir / f"shot{i}.png" for i in range(n)]
        yield temp_files
    finally:
        for file in temp_files:
            if file.exists():
                file.unlink()


async def shot(max_num_monitors: int) -> Image.Image:
    with temp_files(max_num_monitors) as files:
        await run_screencapture(files)
        actual_files = [file for file in files if file.exists()]
        pic = await combine_pics(actual_files)
        return pic
