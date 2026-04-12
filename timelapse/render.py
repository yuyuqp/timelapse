from datetime import datetime
from pathlib import Path
import sys
from timelapse.config import RenderConfig
import anyio
from anyio.streams.text import TextReceiveStream


class Render:
    config: RenderConfig
    rjust_width: int

    def __init__(self, config: RenderConfig):
        self.config = config
        self.rjust_width = self.get_rjust_n()

    def get_rjust_n(self) -> int:
        max_file = max(self.config.pics_dir.iterdir())
        max_counter = int(max_file.stem)
        return len(str(max_counter))

    async def render(self, cmd: list[str]) -> None:
        async with await anyio.open_process(
            cmd,
            cwd=str(self.config.pics_dir),
        ) as process:
            if process.stdout is not None:
                async for text in TextReceiveStream(process.stdout):
                    print(text)
            if process.stderr is not None:
                async for text in TextReceiveStream(process.stderr):
                    print(text, file=sys.stderr)

    def get_render_cmd(self) -> list[str]:
        path_str = str(self.video_path())
        cmd = [
            "ffmpeg",
            "-framerate",
            str(self.config.frame_rate),
            "-i",
            f"%0{self.rjust_width}d.png",
            path_str,
        ]
        return cmd

    def video_path(self) -> Path:
        stem = datetime.now().strftime(r"%Y-%m-%d=%H-%M-%S")
        path = Path(self.config.output_dir, stem + ".mp4")
        return path
