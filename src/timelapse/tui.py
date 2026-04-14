import asyncio
from pathlib import Path
import shlex
import sys

from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import Button, Checkbox, Footer, Header, Input, Label, RichLog


class TimelapseTUI(App[None]):
    CSS = """
    Screen {
        layout: vertical;
    }

    #body {
        layout: horizontal;
        height: 1fr;
    }

    #controls {
        width: 44;
        padding: 1;
        border: tall $accent;
    }

    #log {
        border: tall $primary;
        padding: 0 1;
    }

    .section-title {
        margin-top: 1;
        text-style: bold;
    }

    .action-button {
        width: 1fr;
        margin-top: 1;
    }
    """

    BINDINGS = [
        ("q", "quit", "Quit"),
        ("ctrl+c", "stop_current", "Stop Running"),
    ]

    def __init__(self) -> None:
        super().__init__()
        self._process: asyncio.subprocess.Process | None = None
        self.repo_root = Path(__file__).resolve().parents[2]
        self.main_py = self.repo_root / "main.py"

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal(id="body"):
            with Vertical(id="controls"):
                yield Label("Paths", classes="section-title")
                yield Input(placeholder="Pictures dir (optional for temp clean)", id="pics_dir")
                yield Input(placeholder="Output dir", id="output_dir")

                yield Label("Collect", classes="section-title")
                yield Checkbox("Use temp pictures dir", id="temp_pics")
                yield Checkbox("Continue existing sequence", id="continue_collect")
                yield Checkbox("Current screen only", id="current_screen")
                yield Input(value="6", placeholder="Rate seconds", id="rate")
                yield Input(value="10", placeholder="Rjust width", id="width")
                yield Button("Run Collect", id="run_collect", classes="action-button")

                yield Label("Render", classes="section-title")
                yield Input(value="15", placeholder="Frame rate", id="frame_rate")
                yield Button("Run Render", id="run_render", classes="action-button")

                yield Label("Clean", classes="section-title")
                yield Checkbox("Clean screenshots", id="clean_pics")
                yield Checkbox("Clean videos", id="clean_videos")
                yield Checkbox("I understand files are permanently deleted", id="confirm_clean")
                yield Button("Run Clean", id="run_clean", classes="action-button")

                yield Label("Process", classes="section-title")
                yield Button("Stop Running Command", id="stop_cmd", classes="action-button")

            yield RichLog(id="log", wrap=True, highlight=True, markup=False)
        yield Footer()

    @property
    def log_view(self) -> RichLog:
        return self.query_one("#log", RichLog)

    def _input_value(self, widget_id: str) -> str:
        return self.query_one(f"#{widget_id}", Input).value.strip()

    def _checkbox_value(self, widget_id: str) -> bool:
        return self.query_one(f"#{widget_id}", Checkbox).value

    def _parse_positive_int(self, raw: str, label: str) -> int | None:
        try:
            value = int(raw)
        except ValueError:
            self.notify(f"{label} must be an integer", severity="error")
            return None
        if value <= 0:
            self.notify(f"{label} must be > 0", severity="error")
            return None
        return value

    async def on_button_pressed(self, event: Button.Pressed) -> None:
        button_id = event.button.id
        if button_id == "run_collect":
            cmd = self._build_collect_cmd()
            if cmd is not None:
                self._start_command(cmd)
            return

        if button_id == "run_render":
            cmd = self._build_render_cmd()
            if cmd is not None:
                self._start_command(cmd)
            return

        if button_id == "run_clean":
            cmd = self._build_clean_cmd()
            if cmd is not None:
                self._start_command(cmd)
            return

        if button_id == "stop_cmd":
            self.action_stop_current()

    def _build_collect_cmd(self) -> list[str] | None:
        pics_dir = self._input_value("pics_dir")
        temp_pics = self._checkbox_value("temp_pics")

        if not temp_pics and not pics_dir:
            self.notify("Set pictures dir or enable temp pictures", severity="error")
            return None

        rate = self._parse_positive_int(self._input_value("rate"), "Rate")
        width = self._parse_positive_int(self._input_value("width"), "Rjust width")
        if rate is None or width is None:
            return None

        cmd = [sys.executable, str(self.main_py), "collect", "-r", str(rate), "-w", str(width)]
        if temp_pics:
            cmd.append("--temp-pics")
        else:
            cmd.extend(["-d", pics_dir])

        if self._checkbox_value("continue_collect"):
            cmd.append("--continue")

        if self._checkbox_value("current_screen"):
            cmd.extend(["-s", "current"])
        else:
            cmd.extend(["-s", "all"])

        return cmd

    def _build_render_cmd(self) -> list[str] | None:
        pics_dir = self._input_value("pics_dir")
        output_dir = self._input_value("output_dir")
        if not pics_dir or not output_dir:
            self.notify("Render requires pictures dir and output dir", severity="error")
            return None

        frame_rate = self._parse_positive_int(self._input_value("frame_rate"), "Frame rate")
        if frame_rate is None:
            return None

        return [
            sys.executable,
            str(self.main_py),
            "render",
            "-d",
            pics_dir,
            "-o",
            output_dir,
            "-f",
            str(frame_rate),
        ]

    def _build_clean_cmd(self) -> list[str] | None:
        clean_pics = self._checkbox_value("clean_pics")
        clean_videos = self._checkbox_value("clean_videos")
        if not clean_pics and not clean_videos:
            self.notify("Select at least one clean target", severity="error")
            return None

        if not self._checkbox_value("confirm_clean"):
            self.notify("Tick the clean confirmation checkbox first", severity="warning")
            return None

        pics_dir = self._input_value("pics_dir")
        output_dir = self._input_value("output_dir")

        cmd = [sys.executable, str(self.main_py), "clean", "--yes"]

        if clean_pics:
            cmd.append("--pics")
            if pics_dir:
                cmd.extend(["-d", pics_dir])

        if clean_videos:
            cmd.append("--videos")
            if not output_dir:
                self.notify("Output dir is required when cleaning videos", severity="error")
                return None
            cmd.extend(["-o", output_dir])

        return cmd

    def _start_command(self, cmd: list[str]) -> None:
        if self._process is not None and self._process.returncode is None:
            self.notify("A command is already running", severity="warning")
            return
        self.run_worker(self._run_command(cmd), exclusive=True, thread=False)

    async def _run_command(self, cmd: list[str]) -> None:
        pretty_cmd = " ".join(shlex.quote(part) for part in cmd)
        self.log_view.write(f"$ {pretty_cmd}")

        self._process = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT,
            cwd=str(self.repo_root),
        )

        assert self._process.stdout is not None
        while True:
            line = await self._process.stdout.readline()
            if not line:
                break
            self.log_view.write(line.decode("utf-8", errors="replace").rstrip())

        code = await self._process.wait()
        self.log_view.write(f"[exit {code}]")
        self._process = None

    def action_stop_current(self) -> None:
        if self._process is None or self._process.returncode is not None:
            self.notify("No running command", severity="information")
            return
        self._process.terminate()
        self.log_view.write("[sent terminate]")


def run_tui() -> None:
    app = TimelapseTUI()
    app.run()
