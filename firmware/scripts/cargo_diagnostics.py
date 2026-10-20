"""Checked compiler diagnostics shared by positive and negative Cargo guards."""

from dataclasses import dataclass
import json

from script_values import object_fields, string_value


@dataclass(frozen=True)
class CompilerError:
    message: str
    code: str | None
    rendered: str


def compiler_errors(output: str) -> list[CompilerError]:
    errors: list[CompilerError] = []
    for line in output.splitlines():
        # Cargo documents non-JSON output from other tools on this stream.
        if not line.startswith("{"):
            continue
        try:
            raw: object = json.loads(line)
        except json.JSONDecodeError as error:
            raise AssertionError("malformed Cargo JSON diagnostic") from error
        record = object_fields(raw, "Cargo message must be an object")
        reason = string_value(record.get("reason"), "Cargo message lacks a reason")
        if reason != "compiler-message":
            continue
        message = object_fields(record.get("message"), "invalid Cargo compiler diagnostic")
        level = string_value(message.get("level"), "compiler diagnostic lacks a level")
        text = string_value(message.get("message"), "compiler diagnostic lacks a message")
        if level != "error":
            continue
        raw_code = message.get("code")
        code = (
            None
            if raw_code is None
            else string_value(
                object_fields(raw_code, "invalid compiler error code").get("code"),
                "invalid compiler error code",
            )
        )
        raw_rendered = message.get("rendered")
        rendered = (
            text
            if raw_rendered is None
            else string_value(raw_rendered, "invalid rendered compiler diagnostic")
        )
        errors.append(CompilerError(text, code, rendered))
    return errors
