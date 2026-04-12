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
    counter: int = 0
    limit: int

    def __init__(self, config: CollectConfig):
        self.config = config
        self.counter = self.init_counter()
        self.limit = 10 ** (self.config.rjust_width + 1) - 1

    def init_counter(self) -> int:
        if self.config.start_mode == "continue":
            max_file = max(self.config.pics_dir.iterdir())
            max_counter = int(max_file.stem)
            return max_counter + 1
        else:
            assert self.config.start_mode == "new"
            is_empty = not any(self.config.pics_dir.iterdir())
            if not is_empty:
                files = list(self.config.pics_dir.iterdir())
                logger.warning(f"Directory is not empty, files: {files}")
                raise ValueError("Directory is not empty, do you want to continue?")
            return 0

    def pic_file_path(self) -> Path:
        number_txt = str(self.counter).rjust(self.config.rjust_width, "0")
        return Path(self.config.pics_dir, f"{number_txt}.png")

    async def shot(self):
        path = self.pic_file_path()

        if self.config.platform == "windows":
            pic = await self.shot_windows()
        elif self.config.platform == "mac":
            pic = await self.shot_mac()
        elif self.config.platform == "linux":
            pic = await self.shot_linux()
        else:
            raise ValueError("Unsupported platform")

        await anyio.to_thread.run_sync(pic.save, str(path))
        self.counter += 1
        assert self.counter <= self.limit

    async def shot_windows(self) -> Image:
        all_screens = self.config.screen_mode == "all"
        pic = await anyio.to_thread.run_sync(
            lambda: ImageGrab.grab(
                bbox=None, include_layered_windows=False, all_screens=all_screens
            )
        )
        return pic

    async def shot_mac(self) -> Image:
        max_num_monitors = 5
        if self.config.screen_mode == "current":
            max_num_monitors = 1
        return await shotmac.shot(max_num_monitors)

    async def shot_linux(self) -> Image:
        pic = await anyio.to_thread.run_sync(
            lambda: ImageGrab.grab(bbox=None, include_layered_windows=False)
        )
        if self.config.screen_mode == "current":
            logger.warning("Linux does not support current screen mode in this version")
        return pic


T = TypeVar("T")
AsyncReturn = Coroutine[Any, Any, T]
P = ParamSpec("P")
# HookCallback usually expects an async function that takes something (e.g. a Collect instance) and returns something
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


async def start_loop(
    collect: Collect,
    hooks: Hooks,
):
    if hooks.start is not None:
        await hooks.start(collect)
    while_condition = True
    while while_condition:
        try:
            # async def signal_handler(scope: anyio.CancelScope):
            #     nonlocal while_condition
            #     with anyio.open_signal_receiver(
            #         signal.SIGINT, signal.SIGTERM
            #     ) as signals:
            #         async for _ in signals:
            #             if hooks.on_keyboard_interrupt is None:
            #                 while_condition = False
            #                 scope.cancel()
            #             else:
            #                 should_continue = await hooks.on_keyboard_interrupt(collect)
            #                 if not should_continue:
            #                     while_condition = False
            #                     scope.cancel()
            #             return

            # async with anyio.create_task_group() as tg:
            #     tg.start_soon(signal_handler, tg.cancel_scope)

            if hooks.before_shot is not None:
                await hooks.before_shot(collect)

            await collect.shot()  # shot

            if hooks.after_shot is not None:
                await hooks.after_shot(collect)

            await anyio.sleep(collect.config.rate_secs)  # sleep

        except* KeyboardInterrupt as eg:
            if hooks.on_keyboard_interrupt is None:
                e = None
                for e_ in eg.exceptions:
                    if isinstance(e_, KeyboardInterrupt):
                        e = e_
                    logger.error(e_)
                if e is not None:
                    raise e

            else:
                should_continue = await hooks.on_keyboard_interrupt(collect)
                if not should_continue:
                    while_condition = False
        except* OSError as eg:
            e = None
            for e_ in eg.exceptions:
                if isinstance(e_, OSError):
                    e = e_
                logger.error(e_)
            if e is None:
                pass

            elif "screen grab failed" in e.args:
                if hooks.on_screen_grab_failed is None:
                    pass

                else:
                    should_continue = await hooks.on_screen_grab_failed(collect)
                    if not should_continue:
                        while_condition = False
            else:
                raise e
        except* Exception as eg:
            e = None
            for e_ in eg.exceptions:
                e = e_
                logger.error(e_)
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
