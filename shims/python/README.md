# secchi-analytics for Click

Pure-Python instrumentation for Click CLIs. It records the resolved command
path, explicitly supplied option names, duration, outcome, and execution
context in Secchi's local JSONL spool. Raw argv, positional arguments, option
values (unless allowlisted), and error messages are never recorded.

```python
import click
from secchi_analytics.click import instrument

@click.group()
def cli():
    pass

instrument(cli, app_name="myctl", cli_version="1.2.3")
```

The adapter patches Click's framework dispatch hook, so existing commands and
commands registered later are covered without per-command decorators. Typer
uses Click's command tree; pass the Click command returned by
`typer.main.get_command(app)` to the same function.

Set `SECCHI_ANALYTICS=0` to disable capture or `SECCHI_ANALYTICS_DIR` to
relocate the local data directory.

## Development

```bash
uv sync --project shims/python
uv run --project shims/python pytest
```
