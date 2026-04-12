from dataclasses import dataclass
import logging
from pathlib import Path
from typing import Any, Callable, Coroutine, ParamSpec, TypeVar

import anyio
import anyio.to_thread
from PIL import ImageGrab
from PIL.Image import Image

from timelapse.config import CollectConfig
import timelapse.shotmac as shotmac

logger = logging.getLogger(__name__)


class Collect:
    config: CollectConfig
    counter: int
    limit: int

    def __init__(self, config: CollectConfig):
        self.config = config
        self.counter = self._init_counter()
        self.limit = 10 ** (self.config.rjust_width + 1) - 1

    def _numbered_png_files(self) -> list[Path]:
        return [
            file
            for file in self.config.pics_dir.iterdir()
            if file.is_file() and file.suffix.lower() == ".png" and file.stem.isdigit()
        ]

    def _init_counter(self) -> int:
        self.config.pics_dir.mkdir(parents=True, exist_ok=True)

        files = self._numbered_png_files()
        if self.config.start_mode == "continue":
            if not files:
                return 0
            return max(int(file.stem) for file in files) + 1

        if files:
            raise ValueError("Directory is not empty, use --continue to append frames")
        return 0

    def pic_file_path(self) -> Path:
        number_txt = str(self.counter).rjust(self.config.rjust_width, "0")
        return Path(self.config.pics_dir, f"{number_txt}.png")

    async def shot(self) -> None:
        path = self.pic_file_path()

        if self.config.platform == "windows":
            pic = await self._shot_windows()
        elif self.config.platform == "mac":
            pic = await self._shot_mac()
        elif self.config.platform == "linux":
            pic = await self._shot_linux()
        else:
            raise ValueError("Unsupported platform")

        await anyio.to_thread.run_sync(pic.save, str(path))
        self.counter += 1
        assert self.counter <= self.limit

    async def _shot_windows(self) -> Image:
        all_screens = self.config.screen_mode == "all"
        return await anyio.to_thread.run_sync(
            lambda: ImageGrab.grab(
                bbox=None,
                include_layered_windows=False,
                all_screens=all_screens,
            )
        )

    async def _shot_mac(self) -> Image:
        max_num_monitors = 1 if self.config.screen_mode == "current" else 5
        return await shotmac.shot(max_num_monitors)

    async def _shot_linux(self) -> Image:
        pic = await anyio.to_thread.run_sync(
            lambda: ImageGrab.grab(bbox=None, include_layered_windows=False)
        )
        if self.config.screen_mode == "current":
            logger.warning("Linux does not support current screen mode in this version")
        return pic


T = TypeVar("T")
AsyncReturn = Coroutine[Any, Any, T]
P = ParamSpec("P")
HookCallback = Callable[P, AsyncReturn[T]] | None


@dataclass
class Hooks:
    on_keyboard_interrupt: HookCallback[[Collect], bool] = None
    on_screen_grab_failed: HookCallback[[Collect], bool] = None
    on_error: HookCallback[[Collect, Exception], bool] = None
    start: HookCallback[[Collect], None] = None
    end: HookCallback[[Collect], None] = None
    before_shot: HookCallback[[Collect], None] = None
    after_shot: HookCallback[[Collect], None] = None


async def start_loop(collect: Collect, hooks: Hooks) -> None:
    if hooks.start is not None:
        await hooks.start(collect)

    while_condition = True
    while while_condition:
        try:
            if hooks.before_shot is not None:
                await hooks.before_shot(collect)

            await collect.shot()

            if hooks.after_shot is not None:
                await hooks.after_shot(collect)

            await anyio.sleep(collect.config.rate_secs)

        except* KeyboardInterrupt as eg:
            if hooks.on_keyboard_interrupt is None:
                e = None
                for err in eg.exceptions:
                    if isinstance(err, KeyboardInterrupt):
                        e = err
                    logger.error(err)
                if e is not None:
                    raise e
            else:
                should_continue = await hooks.on_keyboard_interrupt(collect)
                if not should_continue:
                    while_condition = False

        except* OSError as eg:
            e = None
            for err in eg.exceptions:
                if isinstance(err, OSError):
                    e = err
                logger.error(err)

            if e is None:
                pass

            elif "screen grab failed" in e.args:
                if hooks.on_screen_grab_failed is not None:
                    should_continue = await hooks.on_screen_grab_failed(collect)
                    if not should_continue:
                        while_condition = False
            else:
                raise e

        except* Exception as eg:
            e = None
            for err in eg.exceptions:
                e = err
                logger.error(err)

            if e is None:
                pass

            elif hooks.on_error is None:
                raise e

            else:
                should_continue = await hooks.on_error(collect, e)
                if not should_continue:
                    while_condition = False

    if hooks.end is not None:
        await hooks.end(collect)
