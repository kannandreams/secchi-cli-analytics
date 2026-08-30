from __future__ import annotations

import json

import click
from click.testing import CliRunner

from secchi_analytics.click import MemorySink, instrument


def build_cli(sink: MemorySink) -> click.Group:
    @click.group()
    @click.option("--verbose", is_flag=True)
    def cli(verbose: bool) -> None:
        pass

    @cli.group()
    def project() -> None:
        pass

    @project.command()
    @click.argument("name")
    @click.option("--format", "output_format", default="text")
    @click.option("--token")
    def add(name: str, output_format: str, token: str | None) -> None:
        pass

    instrument(
        cli,
        app_name="myctl",
        cli_version="2.3.1",
        allow_values=["output_format", "token"],
        sink=sink,
    )
    return cli


def test_records_resolved_path_explicit_options_and_no_arguments() -> None:
    sink = MemorySink()
    result = CliRunner().invoke(
        build_cli(sink),
        ["--verbose", "project", "add", "private-name", "--format", "json"],
    )

    assert result.exit_code == 0
    assert len(sink.events) == 1
    event = sink.events[0]
    assert event["event_name"] == "myctl.project.add.completed"
    assert event["command_path"] == ["project", "add"]
    assert event["flag_names"] == ["output_format", "verbose"]
    assert event["flag_values"] == {"output_format": "json"}
    serialized = json.dumps(event)
    assert "private-name" not in serialized
    assert "argv" not in serialized


def test_defaults_are_not_reported() -> None:
    sink = MemorySink()
    result = CliRunner().invoke(build_cli(sink), ["project", "add", "demo"])

    assert result.exit_code == 0
    assert sink.events[0]["flag_names"] == []
    assert "flag_values" not in sink.events[0]


def test_redacts_an_allowlisted_secret() -> None:
    sink = MemorySink()
    secret = "ghp_FAKE000000000000FAKE"
    result = CliRunner().invoke(
        build_cli(sink), ["project", "add", "demo", "--token", secret]
    )

    assert result.exit_code == 0
    assert sink.events[0]["flag_values"] == {"token": "[REDACTED:github-token]"}
    assert secret not in json.dumps(sink.events[0])


def test_records_exception_class_but_never_message() -> None:
    sink = MemorySink()

    @click.command()
    def cli() -> None:
        raise ValueError("customer@example.com must not escape")

    instrument(cli, app_name="myctl", sink=sink)
    result = CliRunner().invoke(cli)

    assert result.exit_code == 1
    assert sink.events[0]["event_name"] == "myctl.failed"
    assert sink.events[0]["error_class"] == "ValueError"
    assert "customer@example.com" not in json.dumps(sink.events[0])


def test_preserves_click_exception_exit_code() -> None:
    sink = MemorySink()

    @click.command()
    def cli() -> None:
        raise click.UsageError("unsafe detail")

    instrument(cli, app_name="myctl", sink=sink)
    result = CliRunner().invoke(cli)

    assert result.exit_code == 2
    assert sink.events[0]["exit_code"] == 2
    assert sink.events[0]["error_class"] == "UsageError"
    assert "unsafe detail" not in json.dumps(sink.events[0])


def test_group_result_callback_failure_is_the_invocation_outcome() -> None:
    sink = MemorySink()

    @click.group()
    def cli() -> None:
        pass

    @cli.command()
    def child() -> None:
        pass

    @cli.result_callback()
    def finish(result: object) -> None:
        raise RuntimeError("post-dispatch detail")

    instrument(cli, app_name="myctl", sink=sink)
    result = CliRunner().invoke(cli, ["child"])

    assert result.exit_code == 1
    assert len(sink.events) == 1
    assert sink.events[0]["command_path"] == ["child"]
    assert sink.events[0]["error_class"] == "RuntimeError"


def test_commands_registered_after_instrumentation_are_covered() -> None:
    sink = MemorySink()

    @click.group()
    def cli() -> None:
        pass

    instrument(cli, app_name="myctl", sink=sink)

    @cli.command()
    def later() -> None:
        pass

    result = CliRunner().invoke(cli, ["later"])
    assert result.exit_code == 0
    assert sink.events[0]["command_path"] == ["later"]


def test_sink_failure_never_changes_the_cli_outcome() -> None:
    class BrokenSink:
        def record(self, event: object) -> None:
            raise OSError("disk failed")

    @click.command()
    def cli() -> None:
        click.echo("still works")

    instrument(cli, app_name="myctl", sink=BrokenSink())
    result = CliRunner().invoke(cli)
    assert result.exit_code == 0
    assert result.output == "still works\n"
