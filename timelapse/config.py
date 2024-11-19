from dataclasses import dataclass
from typing import Literal
import os

Platforms = Literal["windows", "mac", "linux"]


@dataclass
class CollectConfig:
    platform: Platforms
    temp_pics: bool
    pics_dir: str
    start_mode: Literal["continue", "new"]
    screen_mode: Literal["all", "current"]
    rate_secs: int


@dataclass
class CompileConfig:
    platform: Platforms
    pics_dir: str
    output_dir: str
    frame_rate: int


def get_platform() -> Platforms:
    if os.name == "nt":
        return "windows"
    elif os.name == "posix":
        if os.uname().sysname == "Darwin":
            return "mac"
        elif os.uname().sysname == "Linux":
            return "linux"
    raise ValueError("Unsupported platform")
