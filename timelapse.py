import gc
import os
import sys
import time
from datetime import datetime
from pathlib import Path
from subprocess import Popen

from PIL import ImageGrab


class Session:
    def __init__(
        self, path=None, output_path=None, rjust_n=None, all_screen=True, numbering=0
    ):
        self.numbering = numbering
        self.path = path
        self.output_path = output_path
        self.rjust_n = 10 if rjust_n is None else rjust_n
        self.limit = 10 ** (self.rjust_n + 1) - 1
        self.all_screen = all_screen

    def get_file_path(self) -> Path:
        number_txt = str(self.numbering).rjust(self.rjust_n, "0")
        return Path(self.path, f"{number_txt}.png")

    def shot(self):
        pic = ImageGrab.grab(
            bbox=None, include_layered_windows=False, all_screens=self.all_screen
        )
        pic.save(str(self.get_file_path()))
        self.numbering += 1
        assert self.numbering <= self.limit

    def compile_video(self, frame_rate=15):
        video_path = Path(
            self.output_path, f'{datetime.now().strftime(r"%Y-%m-%d=%H-%M-%S")}.mp4'
        )
        path_str = str(video_path)
        cmd = [
            "ffmpeg",
            "-framerate",
            str(frame_rate),
            "-i",
            f"%0{self.rjust_n}d.png",
            path_str,
        ]
        process = Popen(cmd, cwd=str(self.path))


if __name__ == "__main__":
    gc.set_threshold(5, 2, 2)
    SLEEP_SECS = 6
    # TODO: read from config file

    path = Path(r"/home/yuyue/Videos/Timelapse/Shots")  # linux
    if os.name == "nt":
        path = Path("C:", "/", "Users", "Yue Yu", "Videos", "Timelapse", "Shots")
    if sys.platform == "darwin":
        path = Path(r"/Users/yuyue/Movies/Timelapse/Shots")

    output_path = Path(r"/home/yuyue/Videos/Timelapse/Compilations")  # linux
    if os.name == "nt":
        output_path = Path(
            "C:", "/", "Users", "Yue Yu", "Videos", "Timelapse", "Compilations"
        )
    if sys.platform == "darwin":
        output_path = Path(r"/Users/yuyue/Movies/Timelapse/Compilations")

    path.mkdir(parents=True, exist_ok=True)
    output_path.mkdir(parents=True, exist_ok=True)

    session = Session(path=path, output_path=output_path)

    if len(sys.argv) > 1:
        if sys.argv[1].lower() in ("c", "compilation"):
            rate = int(input("framerate: "))
            session.compile_video(rate)
            exit(0)
        elif sys.argv[1].lower() in ("r", "rm", "remove", "clear"):
            files = path.glob("**/*")
            for f in files:
                f.unlink()
            exit(0)

    while True:
        try:
            session.shot()
            print(f"{session.numbering}\r", end="")
            time.sleep(SLEEP_SECS)
        except KeyboardInterrupt:
            try:
                print()
                _input = input("Pasued. Continue: [enter], Compile: [c]").lower()
                if _input in ("c", "compile"):
                    rate = int(input("framerate: "))
                    session.compile_video(rate)
                    print("Done")
                    break
                continue
            except KeyboardInterrupt:
                print()
                print("Exiting")
                break
        except OSError as e:
            if "screen grab failed" in e.args:
                continue
            raise (e)

    print()
