"""Core serial session logic — open, read, write, command with response detection."""

import re
import time
import uuid
from dataclasses import dataclass, field

import serial
import serial.tools.list_ports


def list_serial_ports() -> list[dict]:
    """List available serial ports with metadata."""
    ports = []
    for p in serial.tools.list_ports.comports():
        ports.append({
            "port": p.device,
            "description": p.description,
            "hwid": p.hwid,
            "manufacturer": p.manufacturer,
        })
    return ports


def generate_session_id() -> str:
    return uuid.uuid4().hex[:8]


@dataclass
class SerialSession:
    """Manages a single serial port session."""

    port: str
    baud: int = 115200
    bytesize: int = serial.EIGHTBITS
    parity: str = serial.PARITY_NONE
    stopbits: float = serial.STOPBITS_ONE
    echo_filter: bool = True
    session_id: str = field(default_factory=generate_session_id)
    _ser: serial.Serial | None = field(default=None, init=False, repr=False)

    def open(self) -> None:
        if self._ser and self._ser.is_open:
            raise RuntimeError(f"Session {self.session_id} already open on {self.port}")
        self._ser = serial.Serial(
            port=self.port,
            baudrate=self.baud,
            bytesize=self.bytesize,
            parity=self.parity,
            stopbits=self.stopbits,
            timeout=0.1,
        )
        self._ser.reset_input_buffer()

    def close(self) -> None:
        if self._ser and self._ser.is_open:
            self._ser.close()
        self._ser = None

    @property
    def is_open(self) -> bool:
        return self._ser is not None and self._ser.is_open

    def _ensure_open(self) -> serial.Serial:
        if not self.is_open:
            raise RuntimeError(f"Session {self.session_id} is not open")
        assert self._ser is not None
        return self._ser

    def write_raw(self, data: bytes) -> int:
        """Write raw bytes to the port. Returns bytes written."""
        ser = self._ensure_open()
        return ser.write(data)

    def read_output(self, timeout: float = 0.5) -> str:
        """Read pending output using idle timeout. Returns decoded text."""
        ser = self._ensure_open()
        output = b""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            chunk = ser.read(ser.in_waiting or 1)
            if chunk:
                output += chunk
                # Reset deadline on new data (idle timeout)
                deadline = time.monotonic() + timeout
            else:
                time.sleep(0.01)
        return output.decode("utf-8", errors="replace")

    def send_command(
        self,
        command: str,
        timeout: float = 0.5,
        wait_for: str | None = None,
    ) -> str:
        """Send a command and collect the response.

        Args:
            command: Command string to send (\\r\\n appended automatically).
            timeout: Idle timeout in seconds — stop reading after this much
                silence. Also used as overall timeout when wait_for is set.
            wait_for: Optional regex pattern. When matched against accumulated
                output, reading stops immediately (fast path for known prompts).

        Returns:
            Response text, optionally with echoed command stripped.
        """
        ser = self._ensure_open()
        ser.reset_input_buffer()
        ser.write(f"{command}\r\n".encode())

        pattern = re.compile(wait_for) if wait_for else None
        output = b""
        deadline = time.monotonic() + timeout
        max_deadline = time.monotonic() + max(timeout * 20, 10)

        while time.monotonic() < deadline and time.monotonic() < max_deadline:
            chunk = ser.read(ser.in_waiting or 1)
            if chunk:
                output += chunk
                if pattern and pattern.search(output.decode("utf-8", errors="replace")):
                    break
                # Reset idle deadline on new data
                deadline = time.monotonic() + timeout
            else:
                time.sleep(0.01)

        text = output.decode("utf-8", errors="replace")

        if self.echo_filter:
            text = _strip_echo(text, command)

        return text.strip()


def _strip_echo(text: str, command: str) -> str:
    """Remove echoed command from the beginning of output."""
    lines = text.split("\n")
    if lines and command in lines[0]:
        lines = lines[1:]
    return "\n".join(lines)
