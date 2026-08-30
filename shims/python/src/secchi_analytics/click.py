"""One-hook instrumentation for Click command trees (and Typer's Click tree)."""

from __future__ import annotations

import sys
import time
import uuid
from pathlib import Path
from typing import Iterable

import click
from click.core import ParameterSource

from secchi_analytics._capture import (
    EventSink,
    MemorySink,
    SpoolSink,
    build_event,
    detect_context,
    load_or_create_install_id,
    load_settings,
    uuid7,
)

_MARKER = "_secchi_analytics_instrumented"
_STATE_KEY = "secchi.analytics"


def instrument(
    command: click.Command,
    *,
    app_name: str,
    cli_version: str | None = None,
    allow_values: Iterable[str] = (),
    sink: EventSink | None = None,
) -> click.Command:
    """Instrument a Click command tree once and return the same command.

    ``sink`` is intended for tests. When supplied, capture touches neither
    configuration nor disk and events carry an ephemeral install id.
    """

    if not isinstance(command, click.Command):
        raise TypeError(
            "instrument() expects a Click Command; for Typer use "
            "instrument(typer.main.get_command(app), ...)"
        )
    if getattr(command, _MARKER, False):
        return command

    settings = None if sink is not None else load_settings()
    if settings is not None and not settings.enabled:
        setattr(command, _MARKER, True)
        return command

    adapter = _Adapter(
        app_name=app_name,
        cli_version=cli_version,
        allow_values=frozenset(allow_values),
        sink=sink if sink is not None else SpoolSink(settings.spool_dir),
        install_id_path=None if settings is None else settings.install_id_path,
    )
    adapter.patch(command)
    return command


class _Adapter:
    def __init__(
        self,
        *,
        app_name: str,
        cli_version: str | None,
        allow_values: frozenset[str],
        sink: EventSink,
        install_id_path: Path | None,
    ) -> None:
        self.app_name = app_name
        self.cli_version = cli_version
        self.allow_values = allow_values
        self.sink = sink
        self.install_id_path = install_id_path
        self.test_install_id = uuid.uuid4() if install_id_path is None else None

    def patch(self, command: click.Command) -> None:
        if getattr(command, _MARKER, False):
            return
        original_invoke = command.invoke

        def invoke(context: click.Context):
            root = context.find_root()
            state = root.meta.setdefault(
                _STATE_KEY,
                {"started": time.monotonic_ns(), "session_id": uuid7(), "context": root},
            )
            state["context"] = context
            try:
                result = original_invoke(context)
            except BaseException as error:
                if context.parent is None:
                    self._record(
                        state["context"], _exit_code(error), type(error).__name__, state
                    )
                raise
            if context.parent is None:
                self._record(state["context"], 0, None, state)
            return result

        command.invoke = invoke
        setattr(command, _MARKER, True)

        if isinstance(command, click.Group):
            original_add_command = command.add_command

            def add_command(child: click.Command, name: str | None = None) -> None:
                self.patch(child)
                original_add_command(child, name)

            command.add_command = add_command
            for child in command.commands.values():
                self.patch(child)

    def _record(
        self,
        context: click.Context,
        exit_code: int,
        error_class: str | None,
        state: dict[str, object],
    ) -> None:
        try:
            install_id, created = self._install_id()
            if created:
                print(
                    f"{self.app_name}: local usage analytics enabled "
                    "(command names and flag names only, stored in "
                    "~/.secchi/analytics; set SECCHI_ANALYTICS=0 to disable)",
                    file=sys.stderr,
                )
            command_path, flag_names, flag_values = self._invocation(context)
            elapsed = max(time.monotonic_ns() - int(state["started"]), 0) // 1_000_000
            event = build_event(
                cli_name=self.app_name,
                cli_version=self.cli_version,
                command_path=command_path,
                flag_names=flag_names,
                flag_values=flag_values,
                duration_ms=elapsed,
                exit_code=exit_code,
                error_class=error_class,
                context=detect_context(),
                install_id=install_id,
                session_id=state["session_id"],
            )
            self.sink.record(event)
        except BaseException:
            pass

    def _install_id(self) -> tuple[uuid.UUID, bool]:
        if self.install_id_path is None:
            assert self.test_install_id is not None
            return self.test_install_id, False
        return load_or_create_install_id(self.install_id_path)

    def _invocation(
        self, context: click.Context
    ) -> tuple[list[str], list[str], dict[str, str]]:
        contexts: list[click.Context] = []
        current: click.Context | None = context
        while current is not None:
            contexts.append(current)
            current = current.parent
        contexts.reverse()

        command_path = [item.info_name for item in contexts[1:] if item.info_name]
        flag_names: set[str] = set()
        flag_values: dict[str, str] = {}
        for item in contexts:
            for parameter in item.command.params:
                if not isinstance(parameter, click.Option) or parameter.name is None:
                    continue
                if item.get_parameter_source(parameter.name) is not ParameterSource.COMMANDLINE:
                    continue
                flag_names.add(parameter.name)
                value = item.params.get(parameter.name)
                if parameter.name in self.allow_values and isinstance(value, str):
                    flag_values[parameter.name] = value
        return command_path, sorted(flag_names), flag_values


__all__ = ["MemorySink", "instrument"]


def _exit_code(error: BaseException) -> int:
    code = getattr(error, "exit_code", getattr(error, "code", 1))
    return code if isinstance(code, int) else 1
