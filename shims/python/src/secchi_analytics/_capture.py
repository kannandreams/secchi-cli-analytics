"""Event construction and spool capture shared by Python adapters.

This mirrors event schema v1 in ``secchi-analytics-core``. There is no API or
event field for argv, positional arguments, or error messages.
"""

from __future__ import annotations

import json
import math
import os
import platform
import secrets
import sys
import time
import uuid
from collections import Counter
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Mapping, Protocol

from secchi_analytics import __version__

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - exercised on Python 3.10.
    import tomli as tomllib

SCHEMA_VERSION = 1
MAX_LINE_BYTES = 4096
CI_MARKERS = (
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "BUILDKITE",
    "CIRCLECI",
    "TRAVIS",
    "JENKINS_URL",
    "TEAMCITY_VERSION",
)


class EventSink(Protocol):
    """Minimal testing and transport boundary used by the adapter."""

    def record(self, event: Mapping[str, object]) -> None: ...


class MemorySink:
    """In-memory event sink for deterministic integration tests."""

    def __init__(self) -> None:
        self.events: list[dict[str, object]] = []

    def record(self, event: Mapping[str, object]) -> None:
        self.events.append(dict(event))


class SpoolSink:
    """Append one compact JSON event to the UTC day spool."""

    def __init__(self, spool_dir: Path) -> None:
        self.spool_dir = spool_dir

    def record(self, event: Mapping[str, object]) -> None:
        encoded = (
            json.dumps(event, separators=(",", ":"), ensure_ascii=False) + "\n"
        ).encode("utf-8")
        if len(encoded) > MAX_LINE_BYTES:
            return
        day = str(event["ts"])[:10]
        self.spool_dir.mkdir(parents=True, exist_ok=True)
        descriptor = os.open(
            self.spool_dir / f"{day}.jsonl",
            os.O_APPEND | os.O_CREAT | os.O_WRONLY,
            0o600,
        )
        try:
            os.write(descriptor, encoded)
        finally:
            os.close(descriptor)


@dataclass(frozen=True)
class CaptureSettings:
    enabled: bool
    data_dir: Path

    @property
    def install_id_path(self) -> Path:
        return self.data_dir / "install_id"

    @property
    def spool_dir(self) -> Path:
        return self.data_dir / "spool"


@dataclass(frozen=True)
class ExecutionContext:
    actor: str
    agent_name: str | None
    agent_session_id: str | None
    ci: bool
    interactive: bool


def load_settings(env: Mapping[str, str] | None = None) -> CaptureSettings:
    environ = os.environ if env is None else env
    default_dir = _default_data_dir(environ)
    enabled = True
    data_dir = default_dir
    try:
        with (default_dir / "config.toml").open("rb") as handle:
            analytics = tomllib.load(handle).get("analytics", {})
        if isinstance(analytics, dict):
            if isinstance(analytics.get("enabled"), bool):
                enabled = analytics["enabled"]
            if isinstance(analytics.get("data_dir"), str):
                data_dir = Path(analytics["data_dir"])
    except (OSError, tomllib.TOMLDecodeError, AttributeError, TypeError):
        pass

    if environ.get("SECCHI_ANALYTICS_DIR") is not None:
        data_dir = Path(environ["SECCHI_ANALYTICS_DIR"])
    if environ.get("SECCHI_ANALYTICS") in {"0", "false", "off"}:
        enabled = False
    return CaptureSettings(enabled=enabled, data_dir=data_dir)


def detect_context(
    env: Mapping[str, str] | None = None,
    *,
    interactive: bool | None = None,
) -> ExecutionContext:
    environ = os.environ if env is None else env

    def non_empty(name: str) -> str | None:
        return environ.get(name) or None

    explicit = non_empty("SECCHI_ANALYTICS_ACTOR")
    if explicit is not None:
        actor = "agent" if explicit == "agent" else "human"
        agent_name = non_empty("SECCHI_ANALYTICS_AGENT_NAME") if actor == "agent" else None
    elif non_empty("CLAUDECODE") is not None:
        actor, agent_name = "agent", "claude-code"
    else:
        actor, agent_name = "human", None

    if interactive is None:
        interactive = sys.stdin.isatty() and sys.stdout.isatty()
    return ExecutionContext(
        actor=actor,
        agent_name=agent_name,
        agent_session_id=(
            non_empty("SECCHI_ANALYTICS_AGENT_SESSION") if actor == "agent" else None
        ),
        ci=any(non_empty(marker) is not None for marker in CI_MARKERS),
        interactive=interactive,
    )


def load_or_create_install_id(path: Path) -> tuple[uuid.UUID, bool]:
    try:
        return uuid.UUID(path.read_text(encoding="utf-8").strip()), False
    except (OSError, ValueError):
        pass
    path.parent.mkdir(parents=True, exist_ok=True)
    install_id = uuid.uuid4()
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(f"{install_id}\n", encoding="utf-8")
    os.replace(temporary, path)
    return install_id, True


def uuid7(now_ms: int | None = None) -> uuid.UUID:
    """Generate an RFC 9562 UUIDv7 before ``uuid.uuid7`` is universal."""

    timestamp = int(time.time() * 1000) if now_ms is None else now_ms
    random_bits = secrets.randbits(74)
    value = (timestamp & ((1 << 48) - 1)) << 80
    value |= 0x7 << 76
    value |= ((random_bits >> 62) & 0xFFF) << 64
    value |= 0b10 << 62
    value |= random_bits & ((1 << 62) - 1)
    return uuid.UUID(int=value)


def build_event(
    *,
    cli_name: str,
    cli_version: str | None,
    command_path: list[str],
    flag_names: list[str],
    flag_values: Mapping[str, str],
    duration_ms: int,
    exit_code: int,
    error_class: str | None,
    context: ExecutionContext,
    install_id: uuid.UUID,
    session_id: uuid.UUID,
    event_id: uuid.UUID | None = None,
    timestamp: datetime | None = None,
) -> dict[str, object]:
    phase = "completed" if exit_code == 0 and error_class is None else "failed"
    suffix = ".".join([*command_path, phase])
    current = datetime.now(timezone.utc) if timestamp is None else timestamp
    event: dict[str, object] = {
        "event_id": str(uuid7() if event_id is None else event_id),
        "schema_version": SCHEMA_VERSION,
        "event_name": f"{cli_name}.{suffix}",
        "ts": current.astimezone(timezone.utc).isoformat().replace("+00:00", "Z"),
        "install_id": str(install_id),
        "session_id": str(session_id),
        "cli_name": cli_name,
        "command_path": command_path,
        "flag_names": sorted(set(flag_names)),
        "duration_ms": max(duration_ms, 0),
        "exit_code": exit_code,
        "actor": context.actor,
        "ci": context.ci,
        "interactive": context.interactive,
        "os": _rust_os_name(),
        "arch": _rust_arch_name(),
        "sdk_version": __version__,
    }
    if cli_version is not None:
        event["cli_version"] = cli_version
    if flag_values:
        event["flag_values"] = {
            name: redact(value) for name, value in sorted(flag_values.items())
        }
    if error_class is not None:
        event["error_class"] = error_class
    if context.actor == "agent":
        if context.agent_name is not None:
            event["agent_name"] = context.agent_name
        if context.agent_session_id is not None:
            event["agent_session_id"] = context.agent_session_id
    return event


def redact(value: str) -> str:
    label = _secret_label(value)
    return value if label is None else f"[REDACTED:{label}]"


def _secret_label(value: str) -> str | None:
    if (
        len(value) == 20
        and value.startswith("AKIA")
        and all(character.isupper() or character.isdigit() for character in value[4:])
    ):
        return "aws-access-key"
    if value.startswith(("sk-", "sk_")) and len(value) >= 20 and not _has_space(value):
        return "sk-key"
    for prefix in ("ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"):
        rest = value.removeprefix(prefix)
        if rest != value and len(rest) >= 16 and all(c.isalnum() or c == "_" for c in rest):
            return "github-token"
    segments = value.split(".")
    if (
        len(segments) == 3
        and segments[0].startswith("eyJ")
        and all(
            len(segment) >= 8
            and all(c.isalnum() or c in "-_=" for c in segment)
            for segment in segments
        )
    ):
        return "jwt"
    if not _has_space(value) and value.count("@") == 1:
        local, domain = value.split("@")
        labels = domain.split(".")
        if local and len(labels) >= 2 and all(labels):
            return "email"
    if len(value) >= 16 and not _has_space(value):
        mixed_case = any(c.isupper() for c in value) and any(c.islower() for c in value)
        if (any(c.isdigit() for c in value) or mixed_case) and _entropy(value) > 3.7:
            return "high-entropy"
    return None


def _entropy(value: str) -> float:
    total = len(value)
    return sum(
        -(count / total) * math.log2(count / total) for count in Counter(value).values()
    )


def _has_space(value: str) -> bool:
    return any(character.isspace() for character in value)


def _default_data_dir(env: Mapping[str, str]) -> Path:
    home = env.get("USERPROFILE" if os.name == "nt" else "HOME")
    return Path(home) / ".secchi" / "analytics" if home else Path(".secchi-analytics")


def _rust_os_name() -> str:
    return {"darwin": "macos", "win32": "windows"}.get(sys.platform, sys.platform)


def _rust_arch_name() -> str:
    machine = platform.machine().lower()
    return {"arm64": "aarch64", "amd64": "x86_64"}.get(machine, machine)
