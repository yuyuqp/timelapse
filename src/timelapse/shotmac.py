import tempfile
from contextlib import contextmanager
from pathlib import Path

import anyio
import anyio.to_thread
from PIL import Image


async def _run_screencapture(files: list[Path]) -> None:
    cmd = ["screencapture", "-x", *[str(file) for file in files]]
    await anyio.run_process(cmd)


async def _combine_pics(files: list[Path]) -> Image.Image:
    images = await anyio.to_thread.run_sync(lambda: [Image.open(str(file)) for file in files])
    widths, heights = zip(*(img.size for img in images))

    new_image = Image.new("RGB", (sum(widths), max(heights)))
    x_offset = 0
    for image in images:
        new_image.paste(image, (x_offset, 0))
        x_offset += image.size[0]

    return new_image


@contextmanager
def _temp_files(count: int):
    files: list[Path] = []
    try:
        temp_dir = Path(tempfile.gettempdir())
        files = [temp_dir / f"timelapse_shot_{idx}.png" for idx in range(count)]
        yield files
    finally:
        for file in files:
            if file.exists():
                file.unlink()


async def shot(max_num_monitors: int) -> Image.Image:
    with _temp_files(max_num_monitors) as files:
        await _run_screencapture(files)
        existing = [file for file in files if file.exists()]
        if not existing:
            raise OSError("screen grab failed")
        return await _combine_pics(existing)
