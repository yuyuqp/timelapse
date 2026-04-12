import argparse
from pathlib import Path
import tempfile

import anyio

from timelapse.collect import Collect, start_loop, Hooks as CollectHooks
from timelapse.config import CollectConfig, RenderConfig, get_platform
from timelapse.render import Render
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
parser_render.add_argument(
    "-d",
    "--pics-dir",
    action="store",
    required=True,
    help="Directory containing numbered screenshot files",
)
parser_render.add_argument(
    "-o",
    "--output-dir",
    action="store",
    required=True,
    help="Directory where the rendered video is written",
)
parser_render.add_argument(
    "-f",
    "--frame-rate",
    action="store",
    type=int,
    default=15,
    help="Output frame rate",
)

# subcommand clean
parser_clean = subparsers.add_parser(
    "clean", help="Remove the screenshots and the rendered timelapse"
)
parser_clean.add_argument(
    "-d",
    "--pics-dir",
    action="store",
    help="Directory containing screenshots to remove",
)
parser_clean.add_argument(
    "-o",
    "--output-dir",
    action="store",
    help="Directory containing rendered videos to remove",
)
parser_clean.add_argument(
    "--pics-only",
    action="store_true",
    help="Only clean screenshot files",
)
parser_clean.add_argument(
    "--videos-only",
    action="store_true",
    help="Only clean rendered video files",
)


def _assert_existing_dir(path_str: str, arg_name: str) -> Path:
    path = Path(path_str)
    if not path.exists():
        raise ValueError(f"{arg_name} does not exist: {path}")
    if not path.is_dir():
        raise ValueError(f"{arg_name} is not a directory: {path}")
    return path


def _delete_matching_files(path: Path, glob_pattern: str) -> int:
    deleted = 0
    for file in path.glob(glob_pattern):
        if file.is_file():
            file.unlink()
            deleted += 1
    return deleted


def _delete_empty_dirs(path: Path) -> int:
    deleted = 0
    for directory in sorted(path.glob("**/*"), key=lambda p: len(p.parts), reverse=True):
        if directory.is_dir() and not any(directory.iterdir()):
            directory.rmdir()
            deleted += 1
    return deleted


def getRenderConfig(args: argparse.Namespace) -> RenderConfig:
    pics_dir = _assert_existing_dir(args.pics_dir, "--pics-dir")
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    if not any(pics_dir.glob("*.png")):
        raise ValueError(f"No screenshot files found in: {pics_dir}")

    return RenderConfig(
        platform=get_platform(),
        pics_dir=pics_dir,
        output_dir=output_dir,
        frame_rate=args.frame_rate,
    )


def run_clean(args: argparse.Namespace) -> None:
    if args.pics_only and args.videos_only:
        raise ValueError("Cannot combine --pics-only and --videos-only")

    should_clean_pics = not args.videos_only
    should_clean_videos = not args.pics_only

    if should_clean_pics and not args.pics_dir:
        raise ValueError("--pics-dir is required when cleaning screenshots")
    if should_clean_videos and not args.output_dir:
        raise ValueError("--output-dir is required when cleaning videos")

    if should_clean_pics:
        pics_dir = _assert_existing_dir(args.pics_dir, "--pics-dir")
        deleted_pics = _delete_matching_files(pics_dir, "*.png")
        deleted_dirs = _delete_empty_dirs(pics_dir)
        logger.info("Deleted %d screenshots and %d empty directories", deleted_pics, deleted_dirs)

    if should_clean_videos:
        output_dir = _assert_existing_dir(args.output_dir, "--output-dir")
        deleted_videos = _delete_matching_files(output_dir, "*.mp4")
        logger.info("Deleted %d rendered videos", deleted_videos)


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
        render_config = getRenderConfig(args)
        render = Render(render_config)
        cmd = render.get_render_cmd()
        logger.info("Rendering from %s to %s", render_config.pics_dir, render_config.output_dir)
        anyio.run(render.render, cmd)
    elif command == "clean":
        run_clean(args)
    else:
        parser.print_help()
