import argparse
import json
import logging
from pathlib import Path
import tempfile

import anyio

from timelapse.collect import Collect, Hooks as CollectHooks, start_loop
from timelapse.config import CollectConfig, RenderConfig, get_platform
from timelapse.render import Render

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

STATE_DIR = Path.home() / ".timelapse"
STATE_FILE = STATE_DIR / "state.json"


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Cross-platform timelapse tool")
    subparsers = parser.add_subparsers(dest="command")

    parser_collect = subparsers.add_parser("collect", help="Save screenshots for the timelapse")
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
        "--continue",
        action="store_true",
        help="Continue collecting screenshots in an existing folder",
        dest="continue_",
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
        "-r",
        "--rate",
        action="store",
        type=int,
        help="Rate in seconds",
        default=6,
    )
    parser_collect.add_argument(
        "-w",
        "--rjust-width",
        action="store",
        type=int,
        help="Width for zero-padded frame number",
        default=10,
    )

    parser_render = subparsers.add_parser("render", help="Render timelapse from screenshots")
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

    parser_clean = subparsers.add_parser("clean", help="Remove screenshots and rendered videos")
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
        "--pics",
        action="store_true",
        help="Clean screenshot files",
    )
    parser_clean.add_argument(
        "--videos",
        action="store_true",
        help="Clean rendered video files",
    )
    parser_clean.add_argument(
        "-y",
        "--yes",
        action="store_true",
        help="Skip clean confirmation prompt",
    )

    subparsers.add_parser("tui", help="Launch the interactive terminal UI")

    return parser


def _assert_existing_dir(path_str: str, arg_name: str) -> Path:
    path = Path(path_str)
    if not path.exists():
        raise ValueError(f"{arg_name} does not exist: {path}")
    if not path.is_dir():
        raise ValueError(f"{arg_name} is not a directory: {path}")
    return path


def _save_last_temp_pics_dir(path: Path) -> None:
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    state = {"last_temp_pics_dir": str(path)}
    STATE_FILE.write_text(json.dumps(state, indent=2), encoding="utf-8")


def _load_last_temp_pics_dir() -> Path | None:
    if not STATE_FILE.exists():
        return None

    try:
        state = json.loads(STATE_FILE.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        logger.warning("Ignoring invalid state file: %s", STATE_FILE)
        return None

    raw_path = state.get("last_temp_pics_dir")
    if not isinstance(raw_path, str) or not raw_path:
        return None

    path = Path(raw_path)
    if not path.exists() or not path.is_dir():
        logger.warning("Saved temp pictures directory no longer exists: %s", path)
        return None
    return path


def _confirm_clean(pics_dir: Path | None, output_dir: Path | None) -> bool:
    targets: list[str] = []
    if pics_dir is not None:
        targets.append(f"screenshots in {pics_dir}")
    if output_dir is not None:
        targets.append(f"videos in {output_dir}")

    logger.info("About to permanently delete %s", " and ".join(targets))
    try:
        answer = input("Proceed with clean? [y/N]: ").strip().lower()
    except (KeyboardInterrupt, EOFError):
        print()
        return False
    return answer in ("y", "yes")


def _delete_matching_files(path: Path, glob_pattern: str) -> int:
    deleted = 0
    for file in path.glob(glob_pattern):
        if file.is_file():
            file.unlink()
            deleted += 1
    return deleted


def _delete_empty_dirs(path: Path) -> int:
    deleted = 0
    all_dirs = [child for child in path.glob("**/*") if child.is_dir()]
    for directory in sorted(all_dirs, key=lambda p: len(p.parts), reverse=True):
        if not any(directory.iterdir()):
            directory.rmdir()
            deleted += 1
    return deleted


def get_collect_config(args: argparse.Namespace) -> CollectConfig:
    temp_pics = args.temp_pics
    pics_dir_arg = args.pics_dir
    if temp_pics and pics_dir_arg:
        raise ValueError("Cannot use --temp-pics and --pics-dir together")
    if not temp_pics and not pics_dir_arg:
        raise ValueError("Must use either --temp-pics or --pics-dir")

    if temp_pics:
        pics_dir_arg = tempfile.mkdtemp()
        _save_last_temp_pics_dir(Path(pics_dir_arg))
        logger.info("Saved temp pictures directory for future clean: %s", pics_dir_arg)

    assert pics_dir_arg is not None
    pics_dir = Path(pics_dir_arg)

    return CollectConfig(
        platform=get_platform(),
        temp_pics=temp_pics,
        pics_dir=pics_dir,
        start_mode="continue" if args.continue_ else "new",
        screen_mode=args.screen_mode,
        rate_secs=args.rate,
        rjust_width=args.rjust_width,
    )


def get_render_config(args: argparse.Namespace) -> RenderConfig:
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


async def _on_keyboard_interrupt(_: Collect) -> bool:
    logger.info("Keyboard interrupt")
    try:
        user_input = input("Continue: [enter], Stop: [s]: ").lower().strip()
    except KeyboardInterrupt:
        logger.info("Keyboard interrupt again. Stopping gracefully")
        return False

    if user_input in ("", "continue", "c"):
        logger.info("Continuing")
        return True
    if user_input in ("s", "stop"):
        logger.info("Stopping")
        return False

    logger.info("Invalid input")
    return await _on_keyboard_interrupt(_)


def run_clean(args: argparse.Namespace) -> None:
    should_clean_pics = args.pics
    should_clean_videos = args.videos

    if not should_clean_pics and not should_clean_videos:
        raise ValueError("Select at least one clean target: --pics and/or --videos")

    pics_dir: Path | None = None
    output_dir: Path | None = None

    if should_clean_pics and not args.pics_dir:
        saved_pics_dir = _load_last_temp_pics_dir()
        if saved_pics_dir is None:
            raise ValueError(
                "--pics-dir is required when cleaning screenshots unless a temp directory was previously saved"
            )
        pics_dir = saved_pics_dir
        logger.info("Using saved temp pictures directory: %s", pics_dir)

    if should_clean_videos and not args.output_dir:
        raise ValueError("--output-dir is required when cleaning videos")

    if should_clean_pics and pics_dir is None:
        pics_dir = _assert_existing_dir(args.pics_dir, "--pics-dir")

    if should_clean_videos:
        output_dir = _assert_existing_dir(args.output_dir, "--output-dir")

    if not args.yes:
        if not _confirm_clean(pics_dir if should_clean_pics else None, output_dir if should_clean_videos else None):
            logger.info("Clean cancelled")
            return

    if should_clean_pics:
        assert pics_dir is not None
        deleted_pics = _delete_matching_files(pics_dir, "**/*.png")
        deleted_dirs = _delete_empty_dirs(pics_dir)
        logger.info("Deleted %d screenshots and %d empty directories", deleted_pics, deleted_dirs)

    if should_clean_videos:
        assert output_dir is not None
        deleted_videos = _delete_matching_files(output_dir, "**/*.mp4")
        logger.info("Deleted %d rendered videos", deleted_videos)


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()

    if args.command == "collect":
        collect = Collect(get_collect_config(args))

        async def _start(collect: Collect) -> None:
            logger.info("Starting, saving screenshots to %s", collect.config.pics_dir)

        async def _after_shot(collect: Collect) -> None:
            print(f"Shot {collect.counter - 1}", end="\r")

        async def _end(collect: Collect) -> None:
            print()
            logger.info("Ending, saved screenshots to %s", collect.config.pics_dir)

        anyio.run(
            start_loop,
            collect,
            CollectHooks(
                start=_start,
                on_keyboard_interrupt=_on_keyboard_interrupt,
                after_shot=_after_shot,
                end=_end,
            ),
        )
        return

    if args.command == "render":
        render = Render(get_render_config(args))
        cmd = render.get_render_cmd()
        logger.info("Rendering from %s to %s", render.config.pics_dir, render.config.output_dir)
        anyio.run(render.render, cmd)
        return

    if args.command == "clean":
        run_clean(args)
        return

    if args.command == "tui":
        from timelapse.tui import run_tui

        run_tui()
        return

    parser.print_help()
