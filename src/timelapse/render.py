from datetime import datetime
from pathlib import Path
import sys

import anyio
from anyio.streams.text import TextReceiveStream

from timelapse.config import RenderConfig


class Render:
    config: RenderConfig
    rjust_width: int

    def __init__(self, config: RenderConfig):
        self.config = config
        self.rjust_width = self.get_rjust_n()

    def _numbered_png_files(self) -> list[Path]:
        files = [
            file
            for file in self.config.pics_dir.iterdir()
            if file.is_file() and file.suffix.lower() == ".png" and file.stem.isdigit()
        ]
        if not files:
            raise ValueError(f"No numbered .png files found in: {self.config.pics_dir}")
        return files

    def get_rjust_n(self) -> int:
        png_files = self._numbered_png_files()
        return max(len(file.stem) for file in png_files)

    def get_start_number(self) -> int:
        png_files = self._numbered_png_files()
        return min(int(file.stem) for file in png_files)

    async def render(self, cmd: list[str]) -> None:
        async with await anyio.open_process(cmd, cwd=str(self.config.pics_dir)) as process:
            if process.stdout is not None:
                async for text in TextReceiveStream(process.stdout):
                    print(text)
            if process.stderr is not None:
                async for text in TextReceiveStream(process.stderr):
                    print(text, file=sys.stderr)

    def get_render_cmd(self) -> list[str]:
        path_str = str(self.video_path())
        start_number = self.get_start_number()
        return [
            "ffmpeg",
            "-framerate",
            str(self.config.frame_rate),
            "-start_number",
            str(start_number),
            "-i",
            f"%0{self.rjust_width}d.png",
            path_str,
        ]

    def video_path(self) -> Path:
        stem = datetime.now().strftime(r"%Y-%m-%d=%H-%M-%S")
        return Path(self.config.output_dir, f"{stem}.mp4")
