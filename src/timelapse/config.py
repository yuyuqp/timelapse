from dataclasses import dataclass
import os
from pathlib import Path
from typing import Literal

Platform = Literal["windows", "mac", "linux"]


@dataclass
class CollectConfig:
    platform: Platform
    temp_pics: bool
    pics_dir: Path
    start_mode: Literal["continue", "new"]
    screen_mode: Literal["all", "current"]
    rate_secs: int
    rjust_width: int


@dataclass
class RenderConfig:
    platform: Platform
    pics_dir: Path
    output_dir: Path
    frame_rate: int


def get_platform() -> Platform:
    if os.name == "nt":
        return "windows"
    if os.name == "posix":
        system = os.uname().sysname
        if system == "Darwin":
            return "mac"
        if system == "Linux":
            return "linux"
    raise ValueError("Unsupported platform")
