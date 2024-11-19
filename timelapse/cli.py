import argparse
import tempfile

from timelapse.config import CollectConfig, get_platform

# parsing

parser = argparse.ArgumentParser(description="Cross-platform timelapse tool")
subparsers = parser.add_subparsers()

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


def getCollectConfig(args: argparse.Namespace) -> CollectConfig:
    temp_pics = args.temp_pics
    pics_dir = args.pics_dir
    if temp_pics and pics_dir:
        raise ValueError("Cannot use --temp-pics and --pics-dir together")
    if not temp_pics and not pics_dir:
        raise ValueError("Must use either --temp-pics or --pics-dir")
    if temp_pics:
        pics_dir = tempfile.gettempdir()
    start_mode = "continue" if args.continue_ else "new"
    screen_mode = args.screen_mode
    rate_secs = args.rate
    return CollectConfig(
        platform=get_platform(),
        temp_pics=temp_pics,
        pics_dir=pics_dir,
        start_mode=start_mode,
        screen_mode=screen_mode,
        rate_secs=rate_secs,
    )


# subcommand render
parser_render = subparsers.add_parser(
    "render", help="Render the timelapse from the screenshots"
)

# subcommand clean
parser_clean = subparsers.add_parser(
    "clean", help="Remove the screenshots and the rendered timelapse"
)

if __name__ == "__main__":
    args = parser.parse_args()
    if collect_args := args.collect:
        collect_config = getCollectConfig(collect_args)
    elif args.render:
        ...
    elif args.clean:
        ...
    else:
        parser.print_help()
