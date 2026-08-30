from __future__ import annotations

import json
import uuid
from datetime import datetime, timezone
from pathlib import Path

from secchi_analytics._capture import (
    ExecutionContext,
    SpoolSink,
    build_event,
    detect_context,
    load_settings,
)


def test_event_matches_rust_v1_golden() -> None:
    event = build_event(
        cli_name="myctl",
        cli_version="2.3.1",
        command_path=["project", "add"],
        flag_names=["verbose", "format"],
        flag_values={"format": "json"},
        duration_ms=142,
        exit_code=1,
        error_class="ConfigError",
        context=ExecutionContext("agent", "claude-code", "sess-42", True, False),
        install_id=uuid.UUID("11111111-2222-4333-8444-555555555555"),
        session_id=uuid.UUID("0198aaaa-0000-7000-8000-000000000002"),
        event_id=uuid.UUID("0198aaaa-0000-7000-8000-000000000001"),
        timestamp=datetime(2026, 8, 14, 12, tzinfo=timezone.utc),
    )

    golden_path = Path(__file__).parents[3] / "core/tests/golden/event_v1.json"
    golden = json.loads(golden_path.read_text(encoding="utf-8"))
    assert event == golden


def test_spool_uses_event_utc_day(tmp_path) -> None:
    sink = SpoolSink(tmp_path / "spool")
    event = {"ts": "2026-08-14T00:30:00Z", "event_name": "myctl.completed"}
    sink.record(event)
    assert json.loads((tmp_path / "spool/2026-08-14.jsonl").read_text()) == event


def test_settings_precedence_and_kill_switch(tmp_path) -> None:
    default = tmp_path / ".secchi/analytics"
    default.mkdir(parents=True)
    (default / "config.toml").write_text(
        '[analytics]\nenabled = true\ndata_dir = "/from-file"\n', encoding="utf-8"
    )
    settings = load_settings(
        {
            "HOME": str(tmp_path),
            "SECCHI_ANALYTICS": "0",
            "SECCHI_ANALYTICS_DIR": "/from-env",
        }
    )
    assert settings.enabled is False
    assert str(settings.data_dir) == "/from-env"


def test_actor_is_only_set_from_explicit_markers() -> None:
    human = detect_context({"SHELL": "agent-ish"}, interactive=True)
    assert human.actor == "human"
    assert human.interactive is True

    agent = detect_context(
        {
            "SECCHI_ANALYTICS_ACTOR": "agent",
            "SECCHI_ANALYTICS_AGENT_NAME": "test-agent",
            "SECCHI_ANALYTICS_AGENT_SESSION": "session-9",
            "CI": "1",
        },
        interactive=False,
    )
    assert agent == ExecutionContext("agent", "test-agent", "session-9", True, False)
