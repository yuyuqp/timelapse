import argparse
from pathlib import Path
import tempfile

import anyio

from timelapse.collect import Collect, start_loop, Hooks as CollectHooks
from timelapse.config import CollectConfig, get_platform
import logging

logging.basicConfig(level = logging.INFO)
logger = logging.getLogger(__name__)

# parsing

parser = argparse.ArgumentParser(description="Cross-platform timelapse tool")
subparsers = parser.add_subparsers(dest="command")

# subcommand collect
parser_collect = subparsers.add_parser(
    "collect", help="Save screenshots for the timelapse"
)
parser_collect.add_argument(
    "-t",
    "--temp-pics",
    action="store_true",
    help="Save screenshots in a temporary directory",
)
parser_collect.add_argument(
    "-d",
    "--pics-dir",
    action="store",
    help="Directory to save the screenshots",
)
parser_collect.add_argument(
    "--continue", action="store_true", help="Continue the timelapse", dest="continue_"
)
parser_collect.add_argument(
    "-s",
    "--screen-mode",
    action="store",
    choices=["all", "current"],
    help="Screen mode",
    default="all",
)
parser_collect.add_argument(
    "-r", "--rate", action="store", help="Rate in seconds", default=6
)
parser_collect.add_argument(
    "-w", "--rjust-width", action="store", help="Width for rjust", default=10
)


def getCollectConfig(args: argparse.Namespace) -> CollectConfig:
    temp_pics = args.temp_pics
    pics_dir = args.pics_dir
    if temp_pics and pics_dir:
        raise ValueError("Cannot use --temp-pics and --pics-dir together")
    if not temp_pics and not pics_dir:
        raise ValueError("Must use either --temp-pics or --pics-dir")
    if temp_pics:
        pics_dir = tempfile.mkdtemp()
    pics_dir = Path(pics_dir)
    start_mode = "continue" if args.continue_ else "new"
    screen_mode = args.screen_mode
    rate_secs = args.rate
    rjust_width = args.rjust_width
    return CollectConfig(
        platform=get_platform(),
        temp_pics=temp_pics,
        pics_dir=pics_dir,
        start_mode=start_mode,
        screen_mode=screen_mode,
        rate_secs=rate_secs,
        rjust_width=rjust_width,
    )


async def on_keyboard_interrupt(collect: Collect) -> bool:
    logger.info("Keyboard interrupt")
    _input = ""
    try:
        _input = input("Continue: [continue], Compile: [c], Stop: [s]: ").lower()
    except KeyboardInterrupt:
        logger.info("Keyboard interrupt again. Stopping gracefully")
        return False

    if _input.lower() in ("c", "continue"):
        logger.info("Continuing")
        return True
    elif _input.lower() in ("s", "stop"):
        logger.info("Stopping")
        return False
    else:
        logger.info("Invalid input")
        return await on_keyboard_interrupt(collect)


# subcommand render
parser_render = subparsers.add_parser(
    "render", help="Render the timelapse from the screenshots"
)

# subcommand clean
parser_clean = subparsers.add_parser(
    "clean", help="Remove the screenshots and the rendered timelapse"
)


def main():
    args = parser.parse_args()
    command = args.command
    if command == "collect":
        collect_config = getCollectConfig(args)
        collect = Collect(collect_config)

        async def start(collect: Collect):
            logger.info("Starting, saving screenshots to %s", collect.config.pics_dir)

        async def after_shot(collect: Collect) -> None:
            print(f"Shot {collect.counter - 1}", end="\r")
            # logger.info(f"Shot {collect.counter - 1}")

        async def end(collect: Collect) -> None:
            print()
            logger.info("Ending, saved screenshots to %s", collect.config.pics_dir)

        anyio.run(
            start_loop,
            collect,
            CollectHooks(
                start=start,
                on_keyboard_interrupt=on_keyboard_interrupt,
                after_shot=after_shot,
                end=end,
            ),
        )
    elif command == "render":
        ...
    elif command == "clean":
        ...
    else:
        parser.print_help()
